import { type KeyBinding, type RecordedChord, type Settings, type Shortcut } from "@bindings";
import { commands } from "@ipc/commands";
import { keymapStore } from "@ipc/store";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SHORTCUT_GROUPS, withChord, withDefaults, withoutChord } from "./shortcuts";
import { type UpdateSettings } from "./types";

vi.mock("@ipc/commands", () => ({
  commands: { recordShortcut: vi.fn(), cancelShortcutRecording: vi.fn(async () => ({ status: "ok", data: null })) },
  reportFailure: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const KEYMAP: KeyBinding[] = [
  { shortcut: "newTab", chords: ["Ctrl+T"], defaults: ["Ctrl+T"] },
  { shortcut: "reload", chords: ["Ctrl+R", "F5"], defaults: ["Ctrl+R", "F5"] },
  { shortcut: "find", chords: ["Ctrl+G"], defaults: ["Ctrl+F"] },
];

const settings = (shortcuts: Settings["shortcuts"] = {}) => ({ shortcuts }) as Settings;

describe("the settings helpers", () => {
  it("adds a chord after the ones the action has now", () => {
    expect(withChord(settings(), KEYMAP, "reload", "Ctrl+Alt+R").shortcuts).toEqual({
      reload: ["Ctrl+R", "F5", "Ctrl+Alt+R"],
    });
  });

  it("takes a moved chord from the action the user gave it", () => {
    expect(withChord(settings({ find: ["Ctrl+G"] }), KEYMAP, "newTab", "Ctrl+G").shortcuts).toEqual({
      find: [],
      newTab: ["Ctrl+T", "Ctrl+G"],
    });
  });

  it("removes one chord and leaves the action changed", () => {
    expect(withoutChord(settings(), KEYMAP, "reload", "F5").shortcuts).toEqual({ reload: ["Ctrl+R"] });
  });

  it("puts one action back on its defaults", () => {
    expect(withDefaults(settings({ find: ["Ctrl+G"], reload: [] }), "find").shortcuts).toEqual({ reload: [] });
  });
});

describe("the section", () => {
  it("has a row for every action, once", () => {
    const ids = SHORTCUT_GROUPS.flatMap((group) => group.rows.map((row) => row.id)).filter((id) =>
      id.startsWith("shortcut-"),
    );
    expect(ids).toHaveLength(31);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("ShortcutControl", () => {
  let container: HTMLDivElement;
  let root: Root;
  const recordShortcut = vi.mocked(commands.recordShortcut);
  const cancel = vi.mocked(commands.cancelShortcutRecording);

  beforeEach(() => {
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    keymapStore.set(KEYMAP);
    recordShortcut.mockReset();
    cancel.mockClear();
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  const render = (shortcut: Shortcut, update: UpdateSettings, shortcuts: Settings["shortcuts"] = {}) => {
    const row = SHORTCUT_GROUPS.flatMap((group) => group.rows).find((row) => row.id === `shortcut-${shortcut}`)!;
    act(() =>
      root.render(
        <row.Control
          settings={settings(shortcuts)}
          update={update}
          id="c"
          labelId="l"
          describedBy={undefined}
          disabled={false}
        />,
      ),
    );
  };
  // A button by its accessible name: its label when it has one, else its text.
  const button = (name: string) =>
    [...container.querySelectorAll("button")].find(
      (element) => (element.getAttribute("aria-label") ?? element.textContent) === name,
    )!;
  // Each recording answers with the next of these, then waits forever.
  const answers = (...recorded: (RecordedChord | null)[]) => {
    for (const answer of recorded) {
      recordShortcut.mockResolvedValueOnce({ status: "ok", data: answer });
    }
    recordShortcut.mockReturnValue(new Promise(() => {}));
  };
  const apply = (update: ReturnType<typeof vi.fn<UpdateSettings>>, current: Settings) =>
    (update.mock.calls[0]![0] as (current: Settings) => Settings)(current);

  it("shows each chord, and Reset only once the action is changed", () => {
    render("reload", async () => null);
    expect([...container.querySelectorAll("kbd")].map((kbd) => kbd.textContent)).toEqual(["Ctrl+R", "F5"]);
    expect(container.querySelector('[aria-label="Reset to the default keys"]')).toBeNull();

    render("find", async () => null, { find: ["Ctrl+G"] });
    expect(container.querySelector('[aria-label="Reset to the default keys"]')).not.toBeNull();
  });

  it("says None when the action has no chords", () => {
    keymapStore.set([{ shortcut: "home", chords: [], defaults: ["Alt+Home"] }]);
    render("home", async () => null);
    expect(container.textContent).toContain("None");
  });

  it("adds the chord recorded", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    answers({ chord: "Ctrl+Alt+R", refusal: null });
    render("reload", update);

    await act(async () => button("Add a shortcut").click());

    expect(apply(update, settings()).shortcuts).toEqual({ reload: ["Ctrl+R", "F5", "Ctrl+Alt+R"] });
    expect(container.textContent).not.toContain("Press a shortcut…");
  });

  it("explains a refused chord and goes on recording", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    answers({ chord: "Ctrl+C", refusal: "reserved" });
    render("reload", update);

    await act(async () => button("Add a shortcut").click());

    expect(container.textContent).toContain("Ctrl+C is kept for editing text");
    expect(container.textContent).toContain("Press a shortcut…");
    expect(recordShortcut).toHaveBeenCalledTimes(2);
    expect(update).not.toHaveBeenCalled();
  });

  it("does nothing for a chord the action already has", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    answers({ chord: "F5", refusal: null });
    render("reload", update);

    await act(async () => button("Add a shortcut").click());

    expect(update).not.toHaveBeenCalled();
  });

  it("offers a chord another action uses, and moves it on Use here", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    answers({ chord: "Ctrl+T", refusal: null });
    render("reload", update);

    await act(async () => button("Add a shortcut").click());
    expect(container.textContent).toContain("Ctrl+T is used by New tab.");
    expect(update).not.toHaveBeenCalled();

    act(() => button("Use here").click());
    expect(apply(update, settings()).shortcuts).toEqual({ reload: ["Ctrl+R", "F5", "Ctrl+T"] });
  });

  it("leaves the chord where it is on Cancel", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    answers({ chord: "Ctrl+T", refusal: null });
    render("reload", update);

    await act(async () => button("Add a shortcut").click());
    act(() => button("Cancel").click());

    expect(update).not.toHaveBeenCalled();
    expect(container.textContent).not.toContain("is used by");
  });

  it("stops recording when Rust answers with nothing", async () => {
    answers(null);
    render("reload", async () => null);

    await act(async () => button("Add a shortcut").click());

    expect(container.textContent).not.toContain("Press a shortcut…");
  });

  it("cancels the recording when the button loses focus", async () => {
    answers();
    render("reload", async () => null);

    await act(async () => button("Add a shortcut").click());
    act(() => button("Press a shortcut…").dispatchEvent(new FocusEvent("focusout", { bubbles: true })));

    expect(cancel).toHaveBeenCalledOnce();
    expect(container.textContent).not.toContain("Press a shortcut…");
  });

  it("ignores a chord that arrives after the recording was cancelled", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    let answer = (_: { status: "ok"; data: RecordedChord | null }) => {};
    recordShortcut.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
    render("reload", update);

    await act(async () => button("Add a shortcut").click());
    act(() => button("Press a shortcut…").dispatchEvent(new FocusEvent("focusout", { bubbles: true })));
    await act(async () => answer({ status: "ok", data: { chord: "Ctrl+Alt+R", refusal: null } }));

    expect(update).not.toHaveBeenCalled();
    expect(container.textContent).not.toContain("Press a shortcut…");
  });

  it("cancels a pending recording when the row goes away", async () => {
    answers();
    render("reload", async () => null);

    await act(async () => button("Add a shortcut").click());
    act(() => root.render(null));

    expect(cancel).toHaveBeenCalledOnce();
  });

  it("leaves Rust alone when the row goes away after its recording finished", async () => {
    answers({ chord: "Ctrl+Alt+R", refusal: null });
    render("reload", async () => null);

    await act(async () => button("Add a shortcut").click());
    act(() => root.render(null));

    expect(cancel).not.toHaveBeenCalled();
  });

  it("checks a recorded chord against the keymap as it is when the chord arrives", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    let answer = (_: { status: "ok"; data: RecordedChord | null }) => {};
    recordShortcut.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
    render("reload", update);

    await act(async () => button("Add a shortcut").click());
    // Another window binds Ctrl+Alt+R to Home while this one records.
    act(() => keymapStore.set([...KEYMAP, { shortcut: "home", chords: ["Ctrl+Alt+R"], defaults: ["Alt+Home"] }]));
    await act(async () => answer({ status: "ok", data: { chord: "Ctrl+Alt+R", refusal: null } }));

    expect(container.textContent).toContain("Ctrl+Alt+R is used by Go to the home page.");
    expect(update).not.toHaveBeenCalled();
  });

  it("removes a chord", () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    render("reload", update);

    act(() => button("Remove F5").click());

    expect(apply(update, settings()).shortcuts).toEqual({ reload: ["Ctrl+R"] });
  });
});

describe("ResetAllControl", () => {
  it("is disabled until a shortcut is changed, then clears every change", () => {
    const container = document.createElement("div");
    const root = createRoot(container);
    const { Control } = SHORTCUT_GROUPS[0]!.rows[0]!;
    const update = vi.fn<UpdateSettings>(async () => null);
    const render = (shortcuts: Settings["shortcuts"]) =>
      act(() =>
        root.render(
          <Control
            settings={settings(shortcuts)}
            update={update}
            id="c"
            labelId="l"
            describedBy={undefined}
            disabled={false}
          />,
        ),
      );

    render({});
    expect(container.querySelector("button")!.disabled).toBe(true);

    render({ reload: [] });
    act(() => container.querySelector("button")!.click());
    expect(update).toHaveBeenCalledWith({ shortcuts: {} });
    act(() => root.unmount());
  });
});
