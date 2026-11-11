import type { Bookmarks } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { commands, state, openMenu } = vi.hoisted(() => {
  const ok = () => Promise.resolve({ status: "ok", data: null });
  return {
    commands: {
      navigateTab: vi.fn(ok),
      openTab: vi.fn(ok),
      openWindow: vi.fn(ok),
      addBookmark: vi.fn(ok),
      addBookmarkFolder: vi.fn(ok),
      updateBookmark: vi.fn(ok),
      moveBookmark: vi.fn(ok),
      removeBookmark: vi.fn(ok),
    },
    state: { tree: null as Bookmarks | null, isPrivate: false },
    openMenu: vi.fn(),
  };
});
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({
  useBookmarks: () => state.tree,
  useBrowserState: () => ({ private: state.isPrivate }),
}));
vi.mock("@features/context-menu/store", () => ({ openMenu }));

import { BookmarksPage } from "./bookmarks-page";
import { sampleTree } from "./test-tree";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("BookmarksPage", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("PointerEvent", MouseEvent);
    state.tree = sampleTree();
    state.isPrivate = false;
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  const render = (url = "haku://bookmarks") => act(async () => root.render(<BookmarksPage tabId={7} url={url} />));
  const heading = () => container.querySelector("h1")?.textContent;
  const rows = () => [...container.querySelectorAll(".stack > ul > li")].map((li) => li.textContent);
  const rowButton = (text: string) =>
    [...container.querySelectorAll<HTMLButtonElement>("li > button:first-child")].find((button) =>
      button.textContent?.includes(text),
    )!;
  const field = (name: string) => container.querySelector<HTMLInputElement>(`form input[aria-label="${name}"]`)!;
  const button = (text: string) =>
    [...container.querySelectorAll("button")].find((candidate) => candidate.textContent === text)!;
  const menuItems = () => (openMenu.mock.lastCall![0] as { items: { id: string; onSelect?: () => void }[] }).items;
  const contextMenu = (target: Element) =>
    act(async () => target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true })));
  const type = async (input: HTMLInputElement, value: string) => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      set.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  };
  const click = (target: Element, init: MouseEventInit = {}) =>
    act(async () => target.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1, ...init })));

  it("opens the bookmarks bar, or the folder its address names", async () => {
    await render();
    expect(heading()).toBe("Bookmarks bar");
    expect(rows()).toEqual(["Docs", "Hakuhttps://haku.test/"]);

    await render("haku://bookmarks?folder=3");
    expect(heading()).toBe("Docs");
    expect(rows()).toEqual(["Rust", "MDNhttps://developer.mozilla.org/"]);
  });

  it("falls back to the bar for a folder that is gone or is a link", async () => {
    await render("haku://bookmarks?folder=99");
    expect(heading()).toBe("Bookmarks bar");
    await render("haku://bookmarks?folder=8");
    expect(heading()).toBe("Bookmarks bar");
  });

  it("opens a folder in this tab's address, and a link as the click asks", async () => {
    await render();
    await click(rowButton("Docs"));
    expect(commands.navigateTab).toHaveBeenLastCalledWith(7, "haku://bookmarks?folder=3", "other");

    await click(rowButton("Haku"));
    expect(commands.navigateTab).toHaveBeenLastCalledWith(7, "https://haku.test/", "bookmark");
    await click(rowButton("Haku"), { ctrlKey: true });
    expect(commands.openTab).toHaveBeenLastCalledWith("https://haku.test/", false, "bookmark");
    await click(rowButton("Haku"), { shiftKey: true });
    expect(commands.openWindow).toHaveBeenLastCalledWith("https://haku.test/", null);
  });

  it("searches every folder and shows where each match is", async () => {
    await render();
    await type(container.querySelector<HTMLInputElement>('input[type="search"]')!, "book");
    expect(heading()).toBe("Search results");
    expect(rows()).toEqual(["The BookBookmarks bar › Docs › Rust"]);

    await type(container.querySelector<HTMLInputElement>('input[type="search"]')!, "nothing like it");
    expect(container.textContent).toContain("No bookmarks match.");
  });

  it("adds a page to the open folder, taking a bare host as a web address", async () => {
    await render("haku://bookmarks?folder=3");
    await click([...container.querySelectorAll("button")].find((button) => button.textContent === "Add page…")!);
    await type(field("Name"), " Example ");
    await type(field("Address"), "example.com");
    await act(async () => container.querySelector("form")!.requestSubmit());

    expect(commands.addBookmark).toHaveBeenCalledExactlyOnceWith(3, null, "Example", "https://example.com");
    expect(container.querySelector("form")).toBeNull();
  });

  it("keeps the form when the change is refused", async () => {
    commands.addBookmarkFolder.mockResolvedValueOnce({ status: "error", data: null } as never);
    await render();
    await click([...container.querySelectorAll("button")].find((button) => button.textContent === "Add folder…")!);
    await act(async () => container.querySelector("form")!.requestSubmit());

    expect(commands.addBookmarkFolder).toHaveBeenCalledExactlyOnceWith(1, null, "New folder");
    expect(container.querySelector("form")).not.toBeNull();
  });

  it("edits a link in place from its menu, as one line of two fields and Save", async () => {
    await render();
    await contextMenu(rowButton("Haku"));
    await act(async () => menuItems().find((item) => item.id === "edit")!.onSelect!());

    const form = container.querySelector("form")!;
    expect(form.closest("li")).not.toBeNull();
    expect(form.querySelector("label")).toBeNull();
    expect(form.querySelectorAll("input")).toHaveLength(2);
    expect([...form.querySelectorAll("button")].map((entry) => entry.textContent)).toEqual(["Save"]);
    expect(field("Name").value).toBe("Haku");
    await type(field("Address"), "haku.test/new");
    await act(async () => form.requestSubmit());
    expect(commands.updateBookmark).toHaveBeenCalledExactlyOnceWith(8, "Haku", "https://haku.test/new");
  });

  it("discards an edit in place on Escape", async () => {
    await render();
    await contextMenu(rowButton("Haku"));
    await act(async () => menuItems().find((item) => item.id === "edit")!.onSelect!());
    await type(field("Name"), "Haku home");
    await act(async () => field("Name").dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(container.querySelector("form")).toBeNull();
    expect(commands.updateBookmark).not.toHaveBeenCalled();
  });

  it("opens the add form after the last row, where the new item goes", async () => {
    await render();
    await click(button("Add page…"));
    const items = [...container.querySelectorAll(".stack > ul > li")];
    expect(items).toHaveLength(3);
    expect(items.at(-1)!.querySelector("form")).not.toBeNull();
  });

  it("opens the add form in an empty folder in place of saying it is empty", async () => {
    await render("haku://bookmarks?folder=4");
    expect(container.textContent).toContain("This folder is empty.");
    await click(button("Add folder…"));
    expect(container.textContent).not.toContain("This folder is empty.");
    expect(container.querySelector(".stack > ul > li > form")).not.toBeNull();
  });

  it("keeps adding quiet in the header, with Import and Export in its menu", async () => {
    await render();
    expect(button("Add page…").className).not.toContain("edged");
    expect(button("Add folder…").className).not.toContain("edged");
    expect(button("Import…")).toBeUndefined();

    await click(container.querySelector('header [aria-label="More actions"]')!);
    expect(
      menuItems()
        .map((item) => item.id)
        .filter((id) => !id.includes("inspectInterface")),
    ).toEqual(["import", "export"]);
  });

  it("removes a link at once from its menu", async () => {
    await render();
    await contextMenu(rowButton("Haku"));
    await act(async () => menuItems().find((item) => item.id === "remove")!.onSelect!());
    expect(commands.removeBookmark).toHaveBeenCalledExactlyOnceWith(8);
  });

  it("asks on the row before removing a folder that holds anything", async () => {
    await render();
    await contextMenu(rowButton("Docs"));
    await act(async () => menuItems().find((item) => item.id === "remove")!.onSelect!());

    expect(commands.removeBookmark).not.toHaveBeenCalled();
    expect(container.querySelector('[role="alert"]')?.textContent).toBe(
      "Remove “Docs” and the 2 bookmarks in it? This can't be undone.",
    );
    expect(document.activeElement?.textContent).toBe("Cancel");
    await act(async () =>
      document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })),
    );
    expect(container.querySelector('[role="alert"]')).toBeNull();
    expect(commands.removeBookmark).not.toHaveBeenCalled();

    await contextMenu(rowButton("Docs"));
    await act(async () => menuItems().find((item) => item.id === "remove")!.onSelect!());
    await click(button("Remove"));
    expect(commands.removeBookmark).toHaveBeenCalledExactlyOnceWith(3);
    expect(container.querySelector('[role="alert"]')).toBeNull();
  });

  it("asks where the menu was for a folder removed from the folder tree", async () => {
    await render();
    const docs = [...container.querySelectorAll("nav [data-drop-folder]")].find((entry) =>
      entry.textContent?.includes("Docs"),
    )!;
    await contextMenu(docs);
    const remove = menuItems().find((item) => item.id === "remove") as { confirm?: { message: string } };
    expect(remove.confirm?.message).toBe("Remove “Docs” and the 2 bookmarks in it? This can't be undone.");
  });

  it("names a folder's chevron Expand or Collapse and its folder, apart from the folder's own button", async () => {
    await render();
    const chevrons = () =>
      [...container.querySelectorAll<HTMLButtonElement>("nav button[aria-expanded]")].map((chevron) => [
        chevron.getAttribute("aria-label"),
        chevron.getAttribute("aria-expanded"),
      ]);
    // Bookmarks bar, Docs inside it, and Other bookmarks.
    expect(chevrons()).toEqual([
      ["Collapse: Bookmarks bar", "true"],
      ["Expand: Docs", "false"],
      ["Collapse: Other bookmarks", "true"],
    ]);
    await click(container.querySelectorAll("nav button[aria-expanded]")[1]!);
    expect(chevrons()[1]).toEqual(["Collapse: Docs", "true"]);
  });

  it("shows the generic icon for every link in a private window", async () => {
    await render();
    expect(container.querySelector('img[src="https://haku.test/favicon.ico"]')).not.toBeNull();
    state.isPrivate = true;
    await render("haku://bookmarks?folder=1");
    expect(container.querySelector("img")).toBeNull();
  });

  describe("dragging", () => {
    const pointer = async (target: EventTarget, kind: string, y: number) => {
      const event = new MouseEvent(kind, { bubbles: true, cancelable: true, clientX: 10, clientY: y, button: 0 });
      Object.defineProperties(event, { pointerId: { value: 1 }, isPrimary: { value: true } });
      await act(async () => target.dispatchEvent(event));
    };
    /** Lays the open folder's rows out 40 px tall from the top, and hit-tests them. */
    const layOut = () => {
      const items = [...container.querySelectorAll<HTMLElement>("[data-drop-row]")];
      items.forEach((item, index) =>
        vi.spyOn(item, "getBoundingClientRect").mockReturnValue(new DOMRect(0, index * 40, 300, 40)),
      );
      document.elementFromPoint = (_x: number, y: number) => items[Math.floor(y / 40)] ?? null;
    };

    it("moves a row after a later sibling, counting it among the siblings left", async () => {
      await render();
      layOut();
      const docs = rowButton("Docs");
      await pointer(docs, "pointerdown", 5);
      await pointer(docs, "pointermove", 70);
      expect(container.querySelector('[data-drop-row="8"]')?.className).toContain("after:");
      await pointer(docs, "pointerup", 70);

      // Docs is at 0; after Haku (1) is slot 2, which is 1 once Docs has left.
      expect(commands.moveBookmark).toHaveBeenCalledExactlyOnceWith(3, 1, 1);
      await click(docs);
      expect(commands.navigateTab).not.toHaveBeenCalled();
    });

    it("drops into a folder row's middle", async () => {
      await render();
      layOut();
      const haku = rowButton("Haku");
      await pointer(haku, "pointerdown", 45);
      await pointer(haku, "pointermove", 20);
      await pointer(haku, "pointerup", 20);
      expect(commands.moveBookmark).toHaveBeenCalledExactlyOnceWith(8, 3, null);
    });

    it("does nothing for a press that does not travel, or a drag cancelled with Escape", async () => {
      await render();
      layOut();
      const haku = rowButton("Haku");
      await pointer(haku, "pointerdown", 45);
      await pointer(haku, "pointermove", 47);
      await pointer(haku, "pointerup", 47);
      await pointer(haku, "pointerdown", 45);
      await pointer(haku, "pointermove", 5);
      await act(async () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
      await pointer(haku, "pointerup", 5);
      expect(commands.moveBookmark).not.toHaveBeenCalled();
    });
  });
});
