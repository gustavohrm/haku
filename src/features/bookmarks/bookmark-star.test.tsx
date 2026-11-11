import type { BookmarkEditRequested, Bookmarks } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { commands, state, openMenu, requests } = vi.hoisted(() => {
  const ok = (data: unknown = null) => Promise.resolve({ status: "ok", data });
  return {
    commands: {
      bookmarkTab: vi.fn(() => ok({ id: 8, added: false })),
      updateBookmark: vi.fn(() => ok()),
      moveBookmark: vi.fn(() => ok()),
      removeBookmark: vi.fn(() => ok()),
      addBookmarkFolder: vi.fn(() => ok(21)),
      openTab: vi.fn(() => ok()),
    },
    state: { tree: null as Bookmarks | null },
    openMenu: vi.fn(),
    requests: { handler: null as ((event: { payload: BookmarkEditRequested }) => void) | null },
  };
});
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({ useBookmarks: () => state.tree }));
vi.mock("@features/context-menu/store", () => ({ openMenu }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({}) }));
vi.mock("@bindings", () => ({
  events: {
    bookmarkEditRequested: () => ({
      listen: (handler: typeof requests.handler) => {
        requests.handler = handler;
        return Promise.resolve(() => {
          requests.handler = null;
        });
      },
    }),
  },
}));

import { BookmarkStar } from "./bookmark-star";
import { sampleTree } from "./test-tree";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("BookmarkStar", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "ResizeObserver",
      class {
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

  const render = (url: string, tab = 7) => act(async () => root.render(<BookmarkStar tab={tab} url={url} />));
  const star = () => container.querySelector<HTMLButtonElement>("button")!;
  const editor = () => document.querySelector("section");
  const name = () => editor()!.querySelector("input")!;
  const button = (text: string) =>
    [...editor()!.querySelectorAll("button")].find((element) => element.textContent?.includes(text))!;
  const type = async (input: HTMLInputElement, value: string) => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      set.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  };

  it("is filled while the page is bookmarked", async () => {
    await render("https://haku.test/");
    expect(star().getAttribute("aria-pressed")).toBe("true");
    expect(star().getAttribute("aria-label")).toBe("Edit bookmark for this tab");

    await render("https://elsewhere.test/");
    expect(star().getAttribute("aria-pressed")).toBe("false");
    expect(star().getAttribute("aria-label")).toBe("Bookmark this tab");
  });

  it("bookmarks the tab and edits the bookmark it names", async () => {
    await render("https://haku.test/");
    await act(async () => star().click());

    expect(commands.bookmarkTab).toHaveBeenCalledExactlyOnceWith(7);
    expect(name().value).toBe("Haku");
    expect(button("Bookmarks bar")).toBeDefined();
  });

  it("opens under the star, inside the window", async () => {
    const width = vi.spyOn(document.documentElement, "clientWidth", "get").mockReturnValue(1000);
    try {
      await render("https://haku.test/");
      const at = (right: number) =>
        vi.spyOn(star(), "getBoundingClientRect").mockReturnValue(new DOMRect(right - 24, 40, 24, 24));
      const open = async () => {
        await act(async () => star().click());
        return editor()!.style;
      };

      // At the window's right, it ends where the star does.
      at(900);
      expect((await open()).left).toBe(`${900 - 320}px`);
      await act(async () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));

      // In the tab column, too near the left edge to end there, it stays in.
      at(250);
      expect((await open()).left).toBe("4px");
    } finally {
      width.mockRestore();
    }
  });

  it("keeps the edited name however it closes", async () => {
    await render("https://haku.test/");
    await act(async () => star().click());
    await type(name(), "Haku home");
    await act(async () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));

    expect(editor()).toBeNull();
    expect(commands.updateBookmark).toHaveBeenCalledExactlyOnceWith(8, "Haku home", null);
  });

  it("leaves the name alone when it did not change, and when the bookmark is removed", async () => {
    await render("https://haku.test/");
    await act(async () => star().click());
    await act(async () => button("Done").click());
    expect(commands.updateBookmark).not.toHaveBeenCalled();

    await act(async () => star().click());
    await type(name(), "Gone");
    await act(async () => button("Remove").click());
    expect(commands.removeBookmark).toHaveBeenCalledExactlyOnceWith(8);
    expect(commands.updateBookmark).not.toHaveBeenCalled();
    expect(editor()).toBeNull();
  });

  it("moves the bookmark to the folder chosen, without closing", async () => {
    await render("https://haku.test/");
    await act(async () => star().click());
    await act(async () => button("Bookmarks bar").click());

    const { items } = openMenu.mock.lastCall![0] as {
      items: { id: string; checked?: boolean; onSelect?: () => void }[];
    };
    expect(items.find((item) => item.checked)?.id).toBe("folder:1");
    // The menu's own click is not a click away from the editor.
    await act(async () => document.body.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
    expect(editor()).not.toBeNull();
    await act(async () => items.find((item) => item.id === "folder:4")!.onSelect!());
    expect(commands.moveBookmark).toHaveBeenCalledExactlyOnceWith(8, 4, null);
  });

  const folderButton = () => editor()!.querySelector<HTMLButtonElement>('button[aria-haspopup="menu"]')!;
  const menuItems = () => (openMenu.mock.lastCall![0] as { items: { id: string; onSelect?: () => void }[] }).items;

  it("opens the bookmark's folder on the bookmarks page from the folder menu's Manage bookmarks", async () => {
    await render("https://developer.mozilla.org/");
    commands.bookmarkTab.mockResolvedValueOnce({ status: "ok", data: { id: 7, added: false } });
    await act(async () => star().click());
    await act(async () => folderButton().click());
    // After the folders, before what a development build adds.
    const ids = menuItems().map((item) => item.id);
    expect(ids.slice(ids.indexOf("separator:newFolder"), ids.indexOf("manage") + 1)).toEqual([
      "separator:newFolder",
      "newFolder",
      "manage",
    ]);
    await act(async () => menuItems().find((item) => item.id === "manage")!.onSelect!());

    expect(commands.openTab).toHaveBeenCalledExactlyOnceWith("haku://bookmarks?folder=3", true, "other");
    expect(editor()).toBeNull();
  });

  it("names a new folder in place, and moves the bookmark into it", async () => {
    await render("https://haku.test/");
    await act(async () => star().click());
    await act(async () => button("Bookmarks bar").click());
    await act(async () => menuItems().find((item) => item.id === "newFolder")!.onSelect!());

    const field = editor()!.querySelector<HTMLInputElement>('input[aria-label="New folder name"]')!;
    await type(field, "Reading");
    await act(async () => field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));

    expect(commands.addBookmarkFolder).toHaveBeenCalledExactlyOnceWith(1, null, "Reading");
    expect(commands.moveBookmark).toHaveBeenCalledExactlyOnceWith(8, 21, null);
    expect(editor()).not.toBeNull();
  });

  it("goes back to the folder on Esc while naming one, without closing", async () => {
    await render("https://haku.test/");
    await act(async () => star().click());
    await act(async () => button("Bookmarks bar").click());
    await act(async () => menuItems().find((item) => item.id === "newFolder")!.onSelect!());

    const field = editor()!.querySelector<HTMLInputElement>('input[aria-label="New folder name"]')!;
    await act(async () => field.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));

    expect(editor()).not.toBeNull();
    expect(button("Bookmarks bar")).toBeDefined();
    expect(commands.addBookmarkFolder).not.toHaveBeenCalled();
  });

  it("opens the editor for Ctrl+D on this tab only", async () => {
    await render("https://haku.test/");
    await act(async () => requests.handler!({ payload: { tab: 9, id: 8 } }));
    expect(editor()).toBeNull();

    await act(async () => requests.handler!({ payload: { tab: 7, id: 8 } }));
    expect(name().value).toBe("Haku");
    expect(commands.bookmarkTab).not.toHaveBeenCalled();
  });
});
