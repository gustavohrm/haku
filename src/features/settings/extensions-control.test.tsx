import { type Extension, type Settings } from "@bindings";
import { type MenuRequest } from "@features/context-menu/store";
import { extensionsStore } from "@ipc/store";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type UpdateSettings } from "./types";

const { commands, openMenu } = vi.hoisted(() => ({
  commands: { removeExtension: vi.fn(async () => ({ status: "ok", data: null })), openTab: vi.fn() },
  openMenu: vi.fn(),
}));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@features/context-menu/store", () => ({ openMenu }));

const { ExtensionsControl } = await import("./controls");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const EXTENSION: Extension = {
  id: "abc",
  name: "Reader",
  version: "1.0",
  folder: "reader",
  options: null,
  popup: null,
  icon: null,
};

describe("ExtensionsControl", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    commands.removeExtension.mockClear();
    openMenu.mockClear();
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    extensionsStore.set({
      installed: [EXTENSION],
      failed: [{ folder: "broken", error: { kind: "Unsupported", message: "no manifest" } }],
      error: null,
    });
    const settings = { disabledExtensions: [], pinnedExtensions: [] } as unknown as Settings;
    act(() =>
      root.render(
        <ExtensionsControl
          settings={settings}
          update={vi.fn<UpdateSettings>(async () => null)}
          id="c"
          labelId="l"
          describedBy={undefined}
          disabled={false}
        />,
      ),
    );
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    extensionsStore.set(null);
  });

  const button = (name: string) =>
    [...container.querySelectorAll("button")].find(
      (element) => (element.getAttribute("aria-label") ?? element.textContent) === name,
    );
  const chooseUninstall = () => {
    act(() => button("More for Reader")!.click());
    const request = openMenu.mock.calls[0]![0] as MenuRequest;
    const item = request.items.find((candidate) => candidate.kind !== "separator" && candidate.id === "uninstall");
    act(() => (item as { onSelect: () => void }).onSelect());
  };

  it("asks on the extension's row before uninstalling it", async () => {
    chooseUninstall();

    const question = container.querySelector('[role="alert"]');
    expect(question?.closest("li")?.textContent).toContain("Reader");
    expect(question?.textContent).toContain("Its data in Haku, such as its settings and sign-ins, is deleted with it.");
    expect(document.activeElement?.textContent).toBe("Cancel");
    expect(commands.removeExtension).not.toHaveBeenCalled();

    await act(async () => button("Uninstall")!.click());
    expect(commands.removeExtension).toHaveBeenCalledWith("reader");
    expect(container.querySelector('[role="alert"]')).toBeNull();
  });

  it("keeps the extension on Cancel or Esc", () => {
    chooseUninstall();
    act(() => button("Cancel")!.click());
    expect(container.querySelector('[role="alert"]')).toBeNull();

    chooseUninstall();
    act(() => {
      button("Cancel")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(container.querySelector('[role="alert"]')).toBeNull();
    expect(commands.removeExtension).not.toHaveBeenCalled();
  });

  it("calls a failed folder's action Uninstall, and asks on its row too", async () => {
    act(() => button("Uninstall…")!.click());

    const question = container.querySelector('[role="alert"]');
    expect(question?.textContent).toContain("The folder broken and everything in it is deleted.");
    await act(async () => button("Uninstall")!.click());
    expect(commands.removeExtension).toHaveBeenCalledWith("broken");
  });
});
