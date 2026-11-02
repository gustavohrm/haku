import type { Tab } from "@bindings";
import { barHoldStore } from "@features/bars/holds";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { openExtensionPopup, openTab, resetZoom, historyRuleFor, state } = vi.hoisted(() => {
  const state = { private: false, historyPaused: false, historyRules: [] as unknown[] };
  return {
    openExtensionPopup: vi.fn(),
    openTab: vi.fn(),
    resetZoom: vi.fn(),
    // The first rule stands for the one the backend finds deciding the page.
    historyRuleFor: vi.fn(async () => ({ status: "ok", data: state.historyRules[0] ?? null })),
    state,
  };
});

vi.mock("@bindings", () => ({
  events: {
    addressFocusRequested: () => ({ listen: async () => () => {} }),
    bookmarkEditRequested: () => ({ listen: async () => () => {} }),
  },
}));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ label: "window-1" }) }));
vi.mock("@ipc/commands", () => ({ commands: { openExtensionPopup, openTab, resetZoom, historyRuleFor } }));
vi.mock("@ipc/hooks", () => ({
  useBookmarks: () => null,
  useBrowserState: () => state,
  useExtensions: () => ({
    installed: [
      {
        id: "extension",
        name: "Example extension",
        popup: "chrome-extension://id/popup.html",
        icon: null,
        version: "1.0",
        folder: "example",
        options: null,
      },
    ],
  }),
  useSettings: () => ({
    pinnedExtensions: ["extension"],
    disabledExtensions: [],
    keptSites: [],
    historyEnabled: true,
    historyRules: state.historyRules,
  }),
  useDownloads: () => [],
}));

import { Toolbar } from "./toolbar";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tab = (id: number, zoom = 100): Tab => ({
  id,
  history: { entries: [{ url: "https://example.com/", title: "Example", favicon: null }], index: 0 },
  presence: { status: "live", slot: 1 },
  scroll: { x: 0, y: 0 },
  fixed: false,
  activeAt: 0,
  dialog: null,
  permission: null,
  audible: false,
  zoom,
  relieved: false,
  restoring: false,
});

describe("toolbar extension popups", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    state.private = false;
    state.historyPaused = false;
    state.historyRules = [];
    openExtensionPopup.mockReset();
    openTab.mockClear();
    openExtensionPopup.mockResolvedValue({ status: "ok", data: true });
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
    // Releases a popup's hold, which waits for a click in the chrome.
    document.body.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("shows a zoom other than 100% in the address field, which resets it", async () => {
    await act(async () => root.render(<Toolbar tab={tab(7, 125)} />));
    const zoom = container.querySelector<HTMLButtonElement>('label [aria-label="Zoom 125%. Reset to 100%"]');
    expect(zoom?.textContent).toBe("125%");

    await act(async () => zoom?.click());

    expect(resetZoom).toHaveBeenCalledExactlyOnceWith(7);
  });

  it("shows the bookmark star on a web page only", async () => {
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    expect(container.querySelector('label [aria-label="Bookmark this tab"]')).not.toBeNull();

    const settings: Tab = {
      ...tab(7),
      history: { entries: [{ url: "haku://settings", title: "Settings", favicon: null }], index: 0 },
      presence: { status: "internal" },
    };
    await act(async () => root.render(<Toolbar tab={settings} />));
    expect(container.querySelector('[aria-label="Bookmark this tab"]')).toBeNull();
  });

  it("keeps one site menu button however often the tab changes", async () => {
    const internal: Tab = {
      ...tab(8),
      history: { entries: [{ url: "haku://settings", title: "Settings", favicon: null }], index: 0 },
      presence: { status: "internal" },
    };
    // One render at a time, each committed before the next.
    await [tab(7), internal, tab(9), internal, tab(7), tab(9)].reduce(
      (done, shown) => done.then(() => act(async () => root.render(<Toolbar tab={shown} />))),
      Promise.resolve(),
    );
    expect(container.querySelectorAll('label [aria-label="Site information"]')).toHaveLength(1);
  });

  it("shows no zoom at 100%", async () => {
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    expect(container.querySelector('[aria-label^="Zoom"]')).toBeNull();
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
    expect(document.body.textContent).not.toContain("Show history for this site");
    expect(document.body.textContent).not.toContain("Never record this site");
  });

  it("offers to record a site again once it has a never rule", async () => {
    state.historyRules = [{ pattern: { kind: "site", value: "example.com" }, action: "never" }];
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    await act(async () => container.querySelector<HTMLButtonElement>('[aria-label="Site information"]')?.click());
    expect(document.body.textContent).toContain("Record this site again");
    expect(document.body.textContent).not.toContain("Never record this site");
  });

  it("names another rule that stops the site, and leads to the rules in its place", async () => {
    state.historyRules = [{ pattern: { kind: "wildcard", value: "*.example.com/*" }, action: "never" }];
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    await act(async () => container.querySelector<HTMLButtonElement>('[aria-label="Site information"]')?.click());
    expect(document.body.textContent).toContain("Not recorded: rule");
    expect(document.body.textContent).toContain("*.example.com/*");
    expect(document.body.textContent).not.toContain("Record this site again");
    expect(document.body.textContent).not.toContain("Never record this site");
    const edit = [...document.body.querySelectorAll("button")].find(
      (button) => button.textContent === "Edit history rules",
    );
    await act(async () => edit?.click());
    expect(openTab).toHaveBeenCalledExactlyOnceWith("haku://settings/browsing#historyEnabled", true, "other");
  });

  it("opens the site's history from the site menu, and says when history is paused", async () => {
    state.historyPaused = true;
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    await act(async () => container.querySelector<HTMLButtonElement>('[aria-label="Site information"]')?.click());
    expect(document.body.textContent).toContain("History is paused");
    const show = [...document.body.querySelectorAll("button")].find(
      (button) => button.textContent === "Show history for this site",
    );
    await act(async () => show?.click());
    expect(openTab).toHaveBeenCalledExactlyOnceWith("haku://history?site=example.com", true, "other");
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
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(openExtensionPopup).not.toHaveBeenCalled();
    const menu = container.querySelector<HTMLButtonElement>('[aria-label="Extensions"]')!;
    await act(async () => menu.click());
    const entry = document.querySelector<HTMLButtonElement>('section button[title="Example extension"]')!;
    expect(entry.disabled).toBe(true);
    await act(async () => entry.click());
    expect(menu.getAttribute("aria-expanded")).toBe("true");
    expect(openExtensionPopup).not.toHaveBeenCalled();
  });

  it("holds the toolbar while the site or extensions menu is open", async () => {
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    const site = container.querySelector<HTMLButtonElement>('label [aria-expanded="false"]')!;
    await act(async () => site.click());
    expect(barHoldStore.get().toolbar).toBe(1);
    await act(async () => site.click());
    expect(barHoldStore.get().toolbar).toBe(0);
    const menu = container.querySelector<HTMLButtonElement>('[aria-label="Extensions"]')!;
    await act(async () => menu.click());
    expect(barHoldStore.get().toolbar).toBe(1);
    await act(async () => menu.click());
    expect(barHoldStore.get().toolbar).toBe(0);
  });

  it("holds the toolbar for an extension popup, and lets go if it fails to open", async () => {
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    const button = container.querySelector<HTMLButtonElement>('[aria-label="Example extension"]')!;
    await act(async () => button.click());
    expect(barHoldStore.get().toolbar).toBe(1);
    await act(async () => document.body.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
    expect(barHoldStore.get().toolbar).toBe(0);
    openExtensionPopup.mockResolvedValue({ status: "error", error: { kind: "notFound" } });
    await act(async () => button.click());
    expect(barHoldStore.get().toolbar).toBe(0);
  });

  it("lets go of the toolbar when the click closed the open popup", async () => {
    openExtensionPopup.mockResolvedValue({ status: "ok", data: false });
    await act(async () => root.render(<Toolbar tab={tab(7)} />));
    const button = container.querySelector<HTMLButtonElement>('[aria-label="Example extension"]')!;

    await act(async () => button.click());

    expect(barHoldStore.get().toolbar).toBe(0);
  });
});
