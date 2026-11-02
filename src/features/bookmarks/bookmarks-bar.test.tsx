import type { Bookmarks } from "@bindings";
import type { MenuItem } from "@features/overlays/menu-model";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { commands, state, openMenu, updateSettings, drawn } = vi.hoisted(() => {
  const ok = (data: unknown = null) => Promise.resolve({ status: "ok", data });
  return {
    commands: {
      navigateTab: vi.fn(() => ok()),
      openTab: vi.fn(() => ok()),
      openWindow: vi.fn(() => ok()),
    },
    state: { tree: null as Bookmarks | null },
    openMenu: vi.fn(() => () => {}),
    updateSettings: vi.fn(),
    drawn: [] as (string | null)[],
  };
});
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({
  useBookmarks: () => state.tree,
  useSettings: () => ({ bookmarksBar: "always" }),
  useBrowserState: () => ({ private: false }),
}));
vi.mock("@ipc/store", () => ({ updateSettings, createStore: () => ({}) }));
vi.mock("@features/bars/holds", () => ({ holdBar: () => () => {}, useBarHold: () => {} }));
vi.mock("@features/context-menu/store", () => ({ openMenu }));
vi.mock("@features/overlays/use-overlay", () => ({ useOverlay: () => {} }));
// Counts what each item draws, to see which items are drawn again.
vi.mock("./favicon", () => ({
  Favicon: ({ url }: { url: string | null }) => {
    drawn.push(url);
    return null;
  },
}));

import { BookmarksBar } from "./bookmarks-bar";
import { folder, link, sampleTree } from "./test-tree";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("BookmarksBar", () => {
  let container: HTMLDivElement;
  let root: Root;
  let observed: (() => void) | null;

  beforeEach(() => {
    vi.clearAllMocks();
    drawn.length = 0;
    observed = null;
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: () => void) {
          observed = callback;
        }
        observe() {}
        disconnect() {}
      },
    );
    state.tree = sampleTree();
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
  });

  const render = () => act(async () => root.render(<BookmarksBar tab={7} hold="toolbar" />));
  const item = (text: string) =>
    [...container.querySelectorAll("li button")].find((button) => button.textContent === text) as HTMLButtonElement;
  const press = (element: Element, init: MouseEventInit = {}) =>
    act(async () => {
      element.dispatchEvent(new MouseEvent("click", { bubbles: true, ...init }));
    });
  const lastMenu = () => (openMenu.mock.lastCall as unknown as [{ items: MenuItem[]; label: string }])[0];

  it("shows the bar's links and folders, in order", async () => {
    await render();
    expect([...container.querySelectorAll("li")].map((li) => li.textContent)).toEqual(["Docs", "Haku"]);
    expect(item("Docs").getAttribute("aria-haspopup")).toBe("menu");
  });

  it("opens a link in the focused tab, a background tab, or a new window", async () => {
    await render();
    await press(item("Haku"));
    expect(commands.navigateTab).toHaveBeenCalledExactlyOnceWith(7, "https://haku.test/");

    await press(item("Haku"), { ctrlKey: true });
    await act(async () => {
      item("Haku").dispatchEvent(new MouseEvent("auxclick", { bubbles: true, button: 1 }));
    });
    expect(commands.openTab.mock.calls).toEqual([
      ["https://haku.test/", false],
      ["https://haku.test/", false],
    ]);

    await press(item("Haku"), { shiftKey: true });
    expect(commands.openWindow).toHaveBeenCalledExactlyOnceWith("https://haku.test/", null);
  });

  it("opens a folder as a menu of its contents", async () => {
    await render();
    await press(item("Docs"));
    expect(lastMenu().label).toBe("Docs");
    expect(lastMenu().items.map((entry) => entry.id)).toContain("openAll");
  });

  it("opens the item's menu on a right-click, and the row's on its empty space", async () => {
    await render();
    await act(async () => {
      item("Haku").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    });
    expect(openMenu).toHaveBeenCalledOnce();
    expect(lastMenu().label).toBe("Haku");
    expect(lastMenu().items.map((entry) => entry.id)).toContain("edit");

    await act(async () => {
      container
        .querySelector("[data-bookmarks-bar]")!
        .dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    });
    expect(openMenu).toHaveBeenCalledTimes(2);
    const show = lastMenu().items.find((entry) => entry.id === "show");
    if (show?.kind !== "submenu") {
      throw new Error("no Show bookmarks bar");
    }
    expect(show.items.find((entry) => entry.kind === "radio" && entry.checked)?.id).toBe("always");
  });

  it("lists what does not fit in the » menu, and hides it from the row", async () => {
    // Each item is 100 pixels wide, in a row of 150.
    const widths = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(100);
    const lefts = vi.spyOn(HTMLElement.prototype, "offsetLeft", "get").mockImplementation(function (this: HTMLElement) {
      return [...(this.parentElement?.children ?? [])].indexOf(this) * 100;
    });
    const clientWidths = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(150);
    try {
      await render();
      const more = container.querySelector<HTMLButtonElement>('[aria-label="More bookmarks"]')!;
      expect(more).not.toBeNull();
      expect(container.querySelectorAll("li")[1]!.hasAttribute("inert")).toBe(true);
      await press(more);
      expect(lastMenu().items.map((entry) => entry.id)).toEqual(["link:8"]);

      // Wider, everything fits.
      clientWidths.mockReturnValue(400);
      await act(async () => observed?.());
      expect(container.querySelector('[aria-label="More bookmarks"]')).toBeNull();
    } finally {
      widths.mockRestore();
      lefts.mockRestore();
      clientWidths.mockRestore();
    }
  });

  it("draws again only the item whose bookmark changed", async () => {
    await render();
    expect(drawn).toEqual(["https://haku.test/"]);
    drawn.length = 0;

    // A new tree, as every change brings, with one link renamed and one added.
    state.tree = {
      ...sampleTree(),
      bar: folder(1, "Bookmarks bar", [
        folder(3, "Docs", []),
        link(8, "Haku home", "https://haku.test/"),
        link(9, "New", "https://new.test/"),
      ]),
    };
    await render();
    // A new item is drawn again once measured, before it is painted.
    expect(new Set(drawn)).toEqual(new Set(["https://haku.test/", "https://new.test/"]));
    drawn.length = 0;

    state.tree = { ...state.tree, bar: { ...state.tree.bar } };
    await render();
    expect(drawn).toEqual([]);
  });
});
