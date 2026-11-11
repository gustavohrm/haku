import type { BookmarkNode, Bookmarks } from "@bindings";
import type { MenuItem } from "@features/overlays/menu-model";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { commands } = vi.hoisted(() => ({
  commands: { openTab: vi.fn(), openWindow: vi.fn(), moveBookmark: vi.fn(), removeBookmark: vi.fn() },
}));
vi.mock("@ipc/commands", () => ({ commands }));

import { bookmarkMenu, removeQuestion } from "./bookmark-menu";
import { link, sampleTree } from "./test-tree";
import { locate } from "./tree";

const ids = (items: readonly MenuItem[]) =>
  items.filter((item) => item.kind !== "separator" && item.id !== "inspectInterface").map((item) => item.id);
const moveTargets = (items: readonly MenuItem[]) => {
  const move = items.find((item) => item.id === "moveTo");
  return move?.kind === "submenu" ? move.items.map((item) => (item.kind === "command" ? item.label : "")) : [];
};

describe("bookmark menu", () => {
  beforeEach(() => vi.clearAllMocks());

  it("opens a link elsewhere, privately only from an ordinary window", () => {
    const tree = sampleTree();
    const haku = locate(tree, 8)!.node;
    expect(ids(bookmarkMenu(tree, haku, { isPrivate: false, edit: vi.fn() }))).toEqual([
      "openTab",
      "openWindow",
      "openPrivateWindow",
      "edit",
      "moveTo",
      "remove",
    ]);
    expect(ids(bookmarkMenu(tree, haku, { isPrivate: true, edit: vi.fn() }))).not.toContain("openPrivateWindow");
    // A folder has nothing to open elsewhere.
    expect(ids(bookmarkMenu(tree, locate(tree, 3)!.node, { isPrivate: false, edit: vi.fn() }))).toEqual([
      "edit",
      "moveTo",
      "remove",
    ]);
  });

  it("moves to any folder but its own, or for a folder, itself and what it holds", () => {
    const tree = sampleTree();
    expect(moveTargets(bookmarkMenu(tree, locate(tree, 6)!.node, { isPrivate: false, edit: vi.fn() }))).toEqual([
      "Bookmarks bar",
      "Bookmarks bar › Docs",
      "Other bookmarks",
      "Other bookmarks › Recipes",
    ]);
    expect(moveTargets(bookmarkMenu(tree, locate(tree, 3)!.node, { isPrivate: false, edit: vi.fn() }))).toEqual([
      "Other bookmarks",
      "Other bookmarks › Recipes",
    ]);
  });

  const remove = (tree: Bookmarks, node: BookmarkNode, confirmInRow?: (id: number) => void) => {
    const item = bookmarkMenu(tree, node, {
      isPrivate: false,
      edit: vi.fn(),
      ...(confirmInRow ? { confirmInRow } : {}),
    }).find((entry) => entry.id === "remove");
    if (item?.kind !== "command") {
      throw new Error("no Remove");
    }
    return item;
  };

  it("names removing Remove, for a link and a folder alike", () => {
    const tree = sampleTree();
    expect(remove(tree, locate(tree, 8)!.node).label).toBe("Remove");
    expect(remove(tree, locate(tree, 3)!.node).label).toBe("Remove");
  });

  it("removes a link and an empty folder at once", () => {
    const tree = sampleTree();
    const haku = remove(tree, locate(tree, 8)!.node);
    const recipes = remove(tree, locate(tree, 4)!.node);
    expect(haku.confirm).toBeUndefined();
    expect(recipes.confirm).toBeUndefined();
    haku.onSelect();
    recipes.onSelect();
    expect(commands.removeBookmark.mock.calls).toEqual([[8], [4]]);
  });

  it("asks where the menu was before removing a folder that holds anything, naming how many bookmarks go", () => {
    const tree = sampleTree();
    const docs = remove(tree, locate(tree, 3)!.node);
    expect(docs.confirm).toEqual({
      message: "Remove “Docs” and the 2 bookmarks in it? This can't be undone.",
      action: "Remove",
      destructive: true,
    });
    // The menu runs it only once the question is answered yes.
    docs.onSelect();
    expect(commands.removeBookmark).toHaveBeenCalledExactlyOnceWith(3);
  });

  it("hands the question to the row when the menu was opened from one", () => {
    const tree = sampleTree();
    const confirmInRow = vi.fn();
    const docs = remove(tree, locate(tree, 3)!.node, confirmInRow);
    expect(docs.confirm).toBeUndefined();
    docs.onSelect();
    expect(confirmInRow).toHaveBeenCalledExactlyOnceWith(3);
    expect(commands.removeBookmark).not.toHaveBeenCalled();

    // A link needs no question, so it goes at once even from a row.
    remove(tree, locate(tree, 8)!.node, confirmInRow).onSelect();
    expect(commands.removeBookmark).toHaveBeenCalledExactlyOnceWith(8);
  });

  it("says so when a folder holds only folders, or a single bookmark", () => {
    const tree = sampleTree();
    expect(removeQuestion(tree, locate(tree, 5)!.node)).toBe(
      "Remove “Rust” and the bookmark in it? This can't be undone.",
    );
    const recipes = locate(tree, 4)!.node;
    if ("folder" in recipes.kind) {
      recipes.kind.folder.children.push({ ...link(9, "", ""), kind: { folder: { children: [] } } });
    }
    expect(removeQuestion(tree, recipes)).toBe("Remove “Recipes” and the folders in it? This can't be undone.");
  });
});
