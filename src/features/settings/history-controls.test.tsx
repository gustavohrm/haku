import { type HistoryRule, type Settings } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type ControlProps, type UpdateSettings } from "./types";

vi.mock("@ipc/commands", () => ({
  commands: { countHistory: vi.fn(), deleteHistory: vi.fn(async () => ({ status: "ok", data: 0 })) },
}));
vi.mock("@ipc/store", () => ({ tryUpdateSettings: vi.fn() }));
vi.mock("@features/feedback/feedback", () => ({ reportUnexpected: vi.fn() }));

const { commands } = await import("@ipc/commands");
const { tryUpdateSettings } = await import("@ipc/store");
const { HistoryRetentionControl, HistoryRulesControl } = await import("./history-controls");

const countHistory = vi.mocked(commands.countHistory);
const deleteHistory = vi.mocked(commands.deleteHistory);
const stored = vi.mocked(tryUpdateSettings);

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  vi.clearAllMocks();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const site = (value: string, action: HistoryRule["action"] = "never"): HistoryRule => ({
  pattern: { kind: "site", value },
  action,
});

const render = (
  Control: (props: ControlProps) => React.ReactNode,
  settings: Partial<Settings>,
  update: UpdateSettings,
) =>
  act(() =>
    root.render(
      <Control
        settings={settings as Settings}
        update={update}
        id="c"
        labelId="l"
        describedBy={undefined}
        disabled={false}
      />,
    ),
  );

const button = (name: string) =>
  [...container.querySelectorAll("button")].find(
    (each) => (each.getAttribute("aria-label") ?? each.textContent) === name,
  );

const type = async (value: string) => {
  const input = container.querySelector<HTMLInputElement>("input[type=text]")!;
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => {
    set.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
};

const submit = () =>
  act(async () => {
    container.querySelector("form")!.requestSubmit();
  });

/** The rules a stored change writes, given the rules stored before it. */
const written = (before: HistoryRule[]) => {
  const change = stored.mock.calls.at(-1)?.[0];
  return change?.({ historyRules: before } as unknown as Settings).historyRules;
};

describe("HistoryRulesControl", () => {
  it("reorders a rule from the keyboard and removes one, in the order they are checked", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    const rules = [site("a.test"), site("b.test")];
    render(HistoryRulesControl, { historyRules: rules }, update);

    const handles = container.querySelectorAll<HTMLButtonElement>("button[aria-label^='Drag to reorder']");
    await act(async () => handles[0]!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
    // Up from the top stays where it is, and stores nothing.
    await act(async () => handles[0]!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true })));
    act(() => [...container.querySelectorAll<HTMLButtonElement>("button[aria-label='Remove rule']")][1]?.click());

    const [move, remove] = update.mock.calls.map(([change]) => change as (current: Settings) => Settings);
    const current = { historyRules: rules } as unknown as Settings;
    expect(update).toHaveBeenCalledTimes(2);
    expect(move?.(current).historyRules).toEqual([site("b.test"), site("a.test")]);
    expect(remove?.(current).historyRules).toEqual([site("a.test")]);
  });

  it("changes a rule's action in place", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    const rules = [site("a.test"), site("b.test")];
    render(HistoryRulesControl, { historyRules: rules }, update);

    const action = container.querySelectorAll<HTMLSelectElement>("li select")[1]!;
    action.value = "addressOnly";
    await act(async () => action.dispatchEvent(new Event("change", { bubbles: true })));

    const change = update.mock.calls[0]![0] as (current: Settings) => Settings;
    expect(change({ historyRules: rules } as unknown as Settings).historyRules).toEqual([
      site("a.test"),
      site("b.test", "addressOnly"),
    ]);
  });

  it("shows only the field until something is typed", async () => {
    render(HistoryRulesControl, { historyRules: [] }, vi.fn());
    expect(container.querySelector("form select, form fieldset")).toBeNull();

    await type("b.test");
    expect(container.querySelector<HTMLSelectElement>("form select")?.value).toBe("site");
    expect(container.querySelector("form fieldset")).not.toBeNull();
  });

  it("does nothing on Enter in the empty field", async () => {
    render(HistoryRulesControl, { historyRules: [] }, vi.fn());
    await submit();
    expect(countHistory).not.toHaveBeenCalled();
    expect(stored).not.toHaveBeenCalled();
    expect(container.querySelector("[role=alert]")).toBeNull();
  });

  it("withdraws the offer to delete when the action is changed, so the rule shown is the one saved", async () => {
    countHistory.mockResolvedValue({ status: "ok", data: 42 });
    render(HistoryRulesControl, { historyRules: [] }, vi.fn());

    await type("bank.test");
    await submit();
    expect(container.textContent).toContain("42 recorded visits match this rule.");

    await act(async () => container.querySelector<HTMLInputElement>("input[type=radio][value=always]")!.click());
    expect(container.textContent).not.toContain("42 recorded visits match this rule.");
    expect(stored).not.toHaveBeenCalled();
  });

  it("drops a count that answers after the action was changed", async () => {
    let answer: (result: { status: "ok"; data: number }) => void = () => {};
    countHistory.mockReturnValue(new Promise((resolve) => (answer = resolve)));
    render(HistoryRulesControl, { historyRules: [] }, vi.fn());

    await type("bank.test");
    await submit();
    await act(async () => container.querySelector<HTMLInputElement>("input[type=radio][value=always]")!.click());
    await act(async () => answer({ status: "ok", data: 42 }));

    expect(container.textContent).not.toContain("42 recorded visits match this rule.");
    expect(stored).not.toHaveBeenCalled();
  });

  it("adds a rule at the top with the action chosen, replacing one with the same pattern", async () => {
    stored.mockResolvedValue({ status: "ok", data: {} as Settings });
    render(HistoryRulesControl, { historyRules: [] }, vi.fn());

    await type(" b.test ");
    await act(async () => container.querySelector<HTMLInputElement>("input[type=radio][value=always]")!.click());
    await submit();

    expect(countHistory).not.toHaveBeenCalled();
    expect(written([site("a.test"), site("b.test")])).toEqual([site("b.test", "always"), site("a.test")]);
  });

  it("guesses a regular expression from its slashes, and takes a kind chosen by hand", async () => {
    stored.mockResolvedValue({ status: "ok", data: {} as Settings });
    render(HistoryRulesControl, { historyRules: [] }, vi.fn());

    await type("/^https://admin/");
    expect(container.querySelector<HTMLSelectElement>("form select")?.value).toBe("regex");

    await type("example.com/a");
    const kind = container.querySelector<HTMLSelectElement>("form select")!;
    kind.value = "wildcard";
    await act(async () => kind.dispatchEvent(new Event("change", { bubbles: true })));
    await act(async () => container.querySelector<HTMLInputElement>("input[type=radio][value=always]")!.click());
    await submit();

    expect(written([])).toEqual([{ pattern: { kind: "wildcard", value: "example.com/a" }, action: "always" }]);
  });

  it("offers to delete what a never rule matches, and deletes only when asked", async () => {
    countHistory.mockResolvedValue({ status: "ok", data: 42 });
    stored.mockResolvedValue({ status: "ok", data: {} as Settings });
    render(HistoryRulesControl, { historyRules: [] }, vi.fn());

    await type("bank.test");
    await submit();
    expect(stored).not.toHaveBeenCalled();
    expect(container.textContent).toContain("42 recorded visits match this rule.");

    await act(async () => button("Never record and delete 42 visits")?.click());

    expect(written([])).toEqual([site("bank.test")]);
    expect(deleteHistory).toHaveBeenCalledWith(
      expect.objectContaining({ include: [{ kind: "site", value: "bank.test" }] }),
    );
  });

  it("keeps recorded visits when the rule is only from now on", async () => {
    countHistory.mockResolvedValue({ status: "ok", data: 3 });
    stored.mockResolvedValue({ status: "ok", data: {} as Settings });
    render(HistoryRulesControl, { historyRules: [] }, vi.fn());

    await type("bank.test");
    await submit();
    await act(async () => button("Only from now on")?.click());

    expect(written([])).toEqual([site("bank.test")]);
    expect(deleteHistory).not.toHaveBeenCalled();
  });

  it("shows why a pattern is invalid under the field, and stores nothing", async () => {
    countHistory.mockResolvedValue({ status: "error", error: { kind: "InvalidPattern", message: "unclosed group" } });
    render(HistoryRulesControl, { historyRules: [] }, vi.fn());

    await type("(");
    await submit();

    expect(container.querySelector("[role=alert]")?.textContent).toBe("unclosed group");
    expect(container.querySelector("input[type=text]")?.getAttribute("aria-invalid")).toBe("true");
    expect(stored).not.toHaveBeenCalled();
  });
});

describe("HistoryRetentionControl", () => {
  const choose = (value: string) =>
    act(async () => {
      const select = container.querySelector("select")!;
      select.value = value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });

  it("keeps history longer without asking", async () => {
    const update = vi.fn<UpdateSettings>(async () => null);
    render(HistoryRetentionControl, { historyRetention: "days30" }, update);

    await choose("forever");

    expect(countHistory).not.toHaveBeenCalled();
    expect(update).toHaveBeenCalledWith({ historyRetention: "forever" });
  });

  it("asks before keeping history for less time deletes visits", async () => {
    countHistory.mockResolvedValue({ status: "ok", data: 1200 });
    const update = vi.fn<UpdateSettings>(async () => null);
    render(HistoryRetentionControl, { historyRetention: "forever" }, update);

    await choose("days30");

    expect(update).not.toHaveBeenCalled();
    expect(container.textContent).toContain(`${(1200).toLocaleString()} visits are older than that`);
    await act(async () => button("Delete older visits")?.click());
    expect(update).toHaveBeenCalledWith({ historyRetention: "days30" });
  });

  it("shortens without asking when nothing is old enough to delete", async () => {
    countHistory.mockResolvedValue({ status: "ok", data: 0 });
    const update = vi.fn<UpdateSettings>(async () => null);
    render(HistoryRetentionControl, { historyRetention: "forever" }, update);

    await choose("year");

    expect(update).toHaveBeenCalledWith({ historyRetention: "year" });
  });
});
