import type { MenuItem } from "@features/overlays/menu-model";
import { describe, expect, it, vi } from "vitest";

vi.mock("@ipc/commands", () => ({ commands: {} }));

import { barRowMenu, bookmarksButtonMenu, folderItems, nodeItems, type BarActions } from "./bar-menus";
import { folder, link, sampleTree } from "./test-tree";

const actions = (): BarActions => ({ open: vi.fn(), openAll: vi.fn(), manage: vi.fn() });

// Development builds end every interface menu with Inspect interface.
const ids = (items: readonly MenuItem[]) =>
  items.map((item) => item.id).filter((id) => id !== "inspectInterface" && id !== "separator:inspectInterface");

const choose = (items: readonly MenuItem[], id: string) => {
  const item = items.find((candidate) => candidate.id === id);
  if (!item || item.kind === "separator" || item.kind === "submenu") {
    throw new Error(`no command ${id}`);
  }
  item.onSelect();
};

const submenu = (items: readonly MenuItem[], id: string) => {
  const item = items.find((candidate) => candidate.id === id);
  if (item?.kind !== "submenu") {
    throw new Error(`no submenu ${id}`);
  }
  return item.items;
};

describe("folderItems", () => {
  it("lists links and subfolders, then opens all of its own links", () => {
    const tree = sampleTree();
    const docs = tree.bar.kind.folder!.children[0]!;
    const act = actions();
    const items = folderItems(tree, docs, act);

    expect(ids(items)).toEqual(["folder:5", "link:7", "separator:openAll", "openAll"]);
    expect(items.at(-1)).toMatchObject({ label: "Open all (1)" });
    expect(ids(submenu(items, "folder:5"))).toEqual(["link:6", "separator:openAll", "openAll"]);

    choose(items, "link:7");
    expect(act.open).toHaveBeenCalledExactlyOnceWith("https://developer.mozilla.org/");
    choose(items, "openAll");
    expect(act.openAll).toHaveBeenCalledExactlyOnceWith(["https://developer.mozilla.org/"]);
  });

  it("says a folder is empty, and offers no Open all for folders alone", () => {
    const tree = sampleTree();
    expect(folderItems(tree, folder(9, "Empty", []), actions())).toEqual([
      expect.objectContaining({ id: "empty", disabled: true }),
    ]);
    expect(ids(folderItems(tree, tree.other, actions()))).toEqual(["folder:4"]);
  });
});

describe("nodeItems", () => {
  it("lists what does not fit on the bar, by name or address", () => {
    const tree = sampleTree();
    const items = nodeItems(tree, [link(10, "", "https://untitled.test/")], actions());
    expect(items).toEqual([expect.objectContaining({ id: "link:10", label: "https://untitled.test/" })]);
  });
});

describe("bookmarksButtonMenu", () => {
  it("lists the bar, then Other bookmarks, and Manage bookmarks after a separator", () => {
    const act = actions();
    const items = bookmarksButtonMenu(sampleTree(), act);
    expect(ids(items)).toEqual(["folder:3", "link:8", "separator:other", "other", "separator:manage", "manage"]);
    expect(ids(submenu(items, "other"))).toEqual(["folder:4"]);
    choose(items, "manage");
    expect(act.manage).toHaveBeenCalledOnce();
  });
});

describe("barRowMenu", () => {
  it("adds to the bar, offers the three choices of when it shows, and manages after a separator", () => {
    const row = { add: vi.fn(), setShown: vi.fn(), manage: vi.fn() };
    const items = barRowMenu("newTab", row);
    expect(ids(items)).toEqual(["addLink", "addFolder", "separator:show", "show", "separator:manage", "manage"]);

    const show = submenu(items, "show");
    expect(show.map((item) => item.kind === "radio" && item.checked)).toEqual([false, true, false]);
    choose(show, "never");
    expect(row.setShown).toHaveBeenCalledExactlyOnceWith("never");
    expect(items.slice(0, 2).map((item) => item.kind !== "separator" && item.label)).toEqual([
      "Add page…",
      "Add folder…",
    ]);
    choose(items, "addFolder");
    expect(row.add).toHaveBeenCalledExactlyOnceWith("addFolder");
  });
});
