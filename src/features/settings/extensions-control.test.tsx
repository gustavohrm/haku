import { type Extension, type ExtensionToAdd, type ExtensionUpdateCheck, type Settings } from "@bindings";
import { type MenuRequest } from "@features/context-menu/store";
import { extensionsStore } from "@ipc/store";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type UpdateSettings } from "./types";

const { commands, openMenu, success, info, notifyFailure } = vi.hoisted(() => ({
  commands: {
    removeExtension: vi.fn(async () => ({ status: "ok", data: null })),
    addExtension: vi.fn(async (): Promise<unknown> => ({ status: "ok", data: null })),
    confirmExtension: vi.fn(async (): Promise<unknown> => ({ status: "ok", data: null })),
    addExtensionFromStore: vi.fn(async (): Promise<unknown> => ({ status: "ok", data: null })),
    checkExtensionUpdates: vi.fn(async (): Promise<unknown> => ({ status: "ok", data: null })),
    openTab: vi.fn(),
  },
  openMenu: vi.fn(),
  success: vi.fn(),
  info: vi.fn(),
  notifyFailure: vi.fn(),
}));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@features/context-menu/store", () => ({ openMenu }));
vi.mock("@features/feedback/feedback", () => ({
  feedback: () => ({ success, info }),
  notifyFailure,
  reportUnexpected: vi.fn(),
}));

const { AddExtensionControl, ExtensionUpdatesControl, ExtensionsControl } = await import("./controls");

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
      updates: [],
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
    const request = openMenu.mock.calls.at(-1)![0] as MenuRequest;
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

describe("adding an extension", () => {
  let container: HTMLDivElement;
  let root: Root;
  const settings = { disabledExtensions: [], pinnedExtensions: [] } as unknown as Settings;
  const props = {
    settings,
    update: vi.fn<UpdateSettings>(async () => null),
    id: "c",
    labelId: "l",
    describedBy: undefined,
    disabled: false,
  };

  beforeEach(() => {
    commands.addExtension.mockClear();
    commands.confirmExtension.mockClear();
    success.mockClear();
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    extensionsStore.set({ installed: [EXTENSION], failed: [], error: null, updates: [] });
    act(() =>
      root.render(
        <>
          <AddExtensionControl {...props} />
          <ExtensionsControl {...props} />
        </>,
      ),
    );
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    extensionsStore.set(null);
  });

  const button = (name: string) =>
    [...container.querySelectorAll("button")].find((element) => element.textContent === name);
  const asked = async (toAdd: ExtensionToAdd) => {
    commands.addExtension.mockResolvedValueOnce({ status: "ok", data: toAdd });
    await act(async () => button("File…")!.click());
    return container.querySelector('[role="alert"]');
  };

  it("shows what a new extension may do, and installs it only on Add", async () => {
    const question = await asked({
      token: 4,
      name: "Notes",
      version: "2.1",
      icon: null,
      replaces: null,
      warnings: [{ access: { kind: "allSites" }, new: false }],
    });

    expect(question?.closest("li")).toBeNull();
    expect(question?.textContent).toContain("Add Notes 2.1?");
    expect(question?.textContent).toContain("Read and change all your data on every site");
    expect(question?.textContent).not.toContain("New in this version");
    expect(document.activeElement?.textContent).toBe("Cancel");
    expect(commands.confirmExtension).not.toHaveBeenCalled();
    expect(button("File…")?.disabled).toBe(true);

    commands.confirmExtension.mockResolvedValueOnce({
      status: "ok",
      data: { extension: { ...EXTENSION, name: "Notes", version: "2.1" }, replaced: null },
    });
    await act(async () => button("Add")!.click());
    expect(commands.confirmExtension).toHaveBeenCalledWith(4, true);
    expect(container.querySelector('[role="alert"]')).toBeNull();
    expect(success).toHaveBeenCalledWith({ title: "Notes added" });
  });

  it("says which versions an update went between", async () => {
    await asked({ token: 6, name: "Reader", version: "2.0", icon: null, replaces: EXTENSION, warnings: [] });
    commands.confirmExtension.mockResolvedValueOnce({
      status: "ok",
      data: { extension: { ...EXTENSION, version: "2.0" }, replaced: "1.0" },
    });
    await act(async () => button("Update")!.click());
    expect(success).toHaveBeenCalledWith({ title: "Reader updated", description: expect.stringContaining("1.0") });
  });

  it("asks about an update under Add an extension while a search hides the installed list", async () => {
    act(() => root.render(<AddExtensionControl {...props} />));
    const question = await asked({
      token: 8,
      name: "Reader",
      version: "2.0",
      icon: null,
      replaces: EXTENSION,
      warnings: [],
    });
    expect(question?.textContent).toContain("Update to Reader 2.0?");
    await act(async () => button("Cancel")!.click());
  });

  it("asks about an update on its extension's row, marks what it adds, and Esc deletes it", async () => {
    const question = await asked({
      token: 5,
      name: "Reader",
      version: "2.0",
      icon: null,
      replaces: EXTENSION,
      warnings: [
        { access: { kind: "readHistory" }, new: false },
        { access: { kind: "downloads" }, new: true },
      ],
    });

    expect(question?.closest("li")?.textContent).toContain("Reader");
    expect(question?.textContent).toContain("Update to Reader 2.0?");
    const items = [...question!.querySelectorAll("li")].map((item) => item.textContent);
    expect(items).toEqual(["Read your browsing history", "Manage your downloads · New in this version"]);

    await act(async () => {
      button("Cancel")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(commands.confirmExtension).toHaveBeenCalledWith(5, false);
    expect(container.querySelector('[role="alert"]')).toBeNull();
    expect(button("File…")?.disabled).toBe(false);
    expect(success).not.toHaveBeenCalled();
  });
});

describe("updates from the stores", () => {
  let container: HTMLDivElement;
  let root: Root;
  const update = vi.fn<UpdateSettings>(async () => null);
  const props = {
    settings: { disabledExtensions: [], pinnedExtensions: [], checkExtensionUpdates: true } as unknown as Settings,
    update,
    id: "c",
    labelId: "l",
    describedBy: undefined,
    disabled: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    extensionsStore.set({
      installed: [EXTENSION],
      failed: [],
      error: null,
      updates: [{ id: "abc", folder: "reader", version: "2.0", store: "chrome" }],
    });
    act(() =>
      root.render(
        <>
          <ExtensionUpdatesControl {...props} />
          <ExtensionsControl {...props} />
        </>,
      ),
    );
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    extensionsStore.set(null);
  });

  const button = (name: string) =>
    [...container.querySelectorAll("button")].find((element) => element.textContent === name);
  const check = async (data: ExtensionUpdateCheck) => {
    commands.checkExtensionUpdates.mockResolvedValueOnce({ status: "ok", data });
    await act(async () => button("Check now")!.click());
  };

  it("turns the automatic checks off with the switch", () => {
    const toggle = container.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    expect(toggle.checked).toBe(true);
    act(() => toggle.click());
    expect(update).toHaveBeenCalledWith({ checkExtensionUpdates: false });
  });

  it("says when every extension is up to date", async () => {
    await check({ updated: [], waiting: [], error: null });
    expect(commands.checkExtensionUpdates).toHaveBeenCalledOnce();
    expect(success).toHaveBeenCalledWith({ title: "Your extensions are up to date" });
    expect(button("Check now")?.disabled).toBe(false);
  });

  it("names each extension it updated and counts those waiting", async () => {
    await check({
      updated: [{ extension: { ...EXTENSION, version: "2.0" }, replaced: "1.0" }],
      waiting: [
        { id: "def", folder: "notes", version: "3.0", store: "edge" },
        { id: "ghi", folder: "tabs", version: "1.1", store: "chrome" },
      ],
      error: null,
    });
    expect(success).toHaveBeenCalledWith({ title: "Reader updated", description: expect.stringContaining("1.0") });
    expect(info).toHaveBeenCalledWith(expect.objectContaining({ title: "2 extension updates wait for your review" }));
    expect(success).not.toHaveBeenCalledWith({ title: "Your extensions are up to date" });
  });

  it("reports a store that failed, and not that all are up to date", async () => {
    const error = { kind: "Unsupported", message: "offline" } as const;
    await check({ updated: [], waiting: [], error });
    expect(notifyFailure).toHaveBeenCalledWith(error);
    expect(success).not.toHaveBeenCalled();
  });

  it("shows a waiting update on its row and asks about it there on Review update", async () => {
    const row = button("Review update")!.closest("li")!;
    expect(row.textContent).toContain("Update to 2.0 waiting");

    commands.addExtensionFromStore.mockResolvedValueOnce({
      status: "ok",
      data: {
        token: 9,
        name: "Reader",
        version: "2.0",
        icon: null,
        replaces: EXTENSION,
        warnings: [{ access: { kind: "downloads" }, new: true }],
      },
    });
    await act(async () => button("Review update")!.click());
    expect(commands.addExtensionFromStore).toHaveBeenCalledWith("chrome", "abc");
    const question = container.querySelector('[role="alert"]');
    expect(question?.closest("li")?.textContent).toContain("Reader");
    expect(question?.textContent).toContain("Manage your downloads · New in this version");
    expect(button("Check now")?.disabled).toBe(true);

    await act(async () => button("Cancel")!.click());
    expect(commands.confirmExtension).toHaveBeenCalledWith(9, false);
  });
});
