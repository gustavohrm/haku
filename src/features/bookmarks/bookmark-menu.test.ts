import type { MenuItem } from "@features/overlays/menu-model";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { commands, confirm } = vi.hoisted(() => ({
  commands: { openTab: vi.fn(), openWindow: vi.fn(), moveBookmark: vi.fn(), removeBookmark: vi.fn() },
  confirm: vi.fn(),
}));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@features/feedback/feedback", () => ({ feedback: () => ({ dialog: { confirm } }) }));

import { bookmarkMenu, deleteBookmark } from "./bookmark-menu";
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
      "delete",
    ]);
    expect(ids(bookmarkMenu(tree, haku, { isPrivate: true, edit: vi.fn() }))).not.toContain("openPrivateWindow");
    // A folder has nothing to open elsewhere.
    expect(ids(bookmarkMenu(tree, locate(tree, 3)!.node, { isPrivate: false, edit: vi.fn() }))).toEqual([
      "edit",
      "moveTo",
      "delete",
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

  it("deletes a link and an empty folder at once", async () => {
    const tree = sampleTree();
    await deleteBookmark(tree, locate(tree, 8)!.node);
    await deleteBookmark(tree, locate(tree, 4)!.node);
    expect(confirm).not.toHaveBeenCalled();
    expect(commands.removeBookmark.mock.calls).toEqual([[8], [4]]);
  });

  it("asks before deleting a folder that holds anything, naming how many bookmarks it holds", async () => {
    const tree = sampleTree();
    const docs = locate(tree, 3)!.node;
    confirm.mockResolvedValueOnce(false);
    await deleteBookmark(tree, docs);
    expect(confirm).toHaveBeenLastCalledWith(
      "It holds 2 bookmarks, which are deleted with it. This cannot be undone.",
      expect.objectContaining({ title: "Delete “Docs”?" }),
    );
    expect(commands.removeBookmark).not.toHaveBeenCalled();

    confirm.mockResolvedValueOnce(true);
    await deleteBookmark(tree, docs);
    expect(commands.removeBookmark).toHaveBeenCalledExactlyOnceWith(3);
  });

  it("says so when a folder holds only folders", async () => {
    const tree = sampleTree();
    const recipes = locate(tree, 4)!.node;
    if ("folder" in recipes.kind) {
      recipes.kind.folder.children.push({ ...link(9, "", ""), kind: { folder: { children: [] } } });
    }
    confirm.mockResolvedValueOnce(false);
    await deleteBookmark(tree, recipes);
    expect(confirm).toHaveBeenLastCalledWith(
      "The folders inside it are deleted with it. This cannot be undone.",
      expect.anything(),
    );
  });
});
