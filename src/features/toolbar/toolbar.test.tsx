import type { Tab } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { openExtensionPopup, state } = vi.hoisted(() => ({ openExtensionPopup: vi.fn(), state: { private: false } }));

vi.mock("@bindings", () => ({
  events: { addressFocusRequested: () => ({ listen: async () => () => {} }) },
}));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ label: "window-1" }) }));
vi.mock("@ipc/commands", () => ({ commands: { openExtensionPopup } }));
vi.mock("@ipc/hooks", () => ({
  useBrowserState: () => state,
  useExtensions: () => ({
    installed: [{ id: "extension", name: "Example extension", popup: "chrome-extension://id/popup.html", icon: null }],
  }),
  useSettings: () => ({ pinnedExtensions: ["extension"], disabledExtensions: [], keptSites: [] }),
  useDownloads: () => [],
}));

import { Toolbar } from "./toolbar";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tab = (id: number): Tab => ({
  id,
  history: { entries: [{ url: "https://example.com/", title: "Example", favicon: null }], index: 0 },
  presence: { status: "live", slot: 1 },
  scroll: { x: 0, y: 0 },
  fixed: false,
  activeAt: 0,
  dialog: null,
  audible: false,
  relieved: false,
  restoring: false,
});

describe("toolbar extension popups", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    state.private = false;
    openExtensionPopup.mockClear();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("labels a private window beside the address", async () => {
    state.private = true;
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    expect(container.textContent).toContain("Private");
  });

  it("hides extensions in private windows", async () => {
    state.private = true;
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    expect(container.querySelector('[aria-label="Extensions"]')).toBeNull();
    expect(container.querySelector('[aria-label="Example extension"]')).toBeNull();
  });

  it("hides site-derived settings while retaining the per-tab control", async () => {
    state.private = true;
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    await act(async () => container.querySelector<HTMLButtonElement>('[aria-expanded="false"]')?.click());
    expect(document.body.textContent).toContain("Keep this tab loaded");
    expect(document.body.textContent).not.toContain("Don't unload this site");
  });

  it("passes the focused tab to a pinned extension popup, including after selection changes", async () => {
    const openFor = async (id: number) => {
      await act(async () => root.render(<Toolbar tab={tab(id)} />));
      const button = container.querySelector<HTMLButtonElement>('[aria-label="Example extension"]')!;
      vi.spyOn(button, "getBoundingClientRect").mockReturnValue({
        left: id * 10,
        top: 31,
        width: 28,
        height: 27,
      } as DOMRect);
      await act(async () => button.click());
      expect(openExtensionPopup).toHaveBeenLastCalledWith("extension", id, {
        x: id * 10,
        y: 31,
        width: 28,
        height: 27,
      });
    };
    await openFor(7);
    await openFor(9);
  });

  it("does not open a popup when no focused tab exists", async () => {
    await act(async () => root.render(<Toolbar tab={null} />));
    const button = container.querySelector<HTMLButtonElement>('[aria-label="Example extension"]')!;
    await act(async () => button.click());
    expect(button.disabled).toBe(true);
    expect(openExtensionPopup).not.toHaveBeenCalled();
    const menu = container.querySelector<HTMLButtonElement>('[aria-label="Extensions"]')!;
    await act(async () => menu.click());
    const entry = document.querySelector<HTMLButtonElement>('section button[title="Example extension"]')!;
    expect(entry.disabled).toBe(true);
    await act(async () => entry.click());
    expect(menu.getAttribute("aria-expanded")).toBe("true");
    expect(openExtensionPopup).not.toHaveBeenCalled();
  });
});
