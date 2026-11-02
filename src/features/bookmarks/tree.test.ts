import { describe, expect, it } from "vitest";

import { folder, link, sampleTree } from "./test-tree";
import {
  addressOf,
  contains,
  countLinks,
  displayTitle,
  faviconOf,
  isBookmarkable,
  listFolders,
  locate,
  moveIndex,
  newestLinkTo,
  search,
} from "./tree";

describe("bookmark tree", () => {
  it("names the roots in the interface's language and untitled links by their URL", () => {
    const tree = sampleTree();
    expect(displayTitle(tree, tree.bar)).toBe("Bookmarks bar");
    expect(displayTitle(tree, tree.other)).toBe("Other bookmarks");
    expect(displayTitle(tree, link(9, "", "https://a.test/"))).toBe("https://a.test/");
  });

  it("locates a node with its parent and ancestors", () => {
    const found = locate(sampleTree(), 6);
    expect(found?.parent?.id).toBe(5);
    expect(found?.ancestors.map((folder) => folder.id)).toEqual([1, 3, 5]);
    expect(locate(sampleTree(), 2)?.parent).toBeNull();
    expect(locate(sampleTree(), 99)).toBeNull();
  });

  it("lists folders depth first with their paths", () => {
    expect(listFolders(sampleTree()).map(({ folder, depth, path }) => [folder.id, depth, path.join("/")])).toEqual([
      [1, 0, "Bookmarks bar"],
      [3, 1, "Bookmarks bar/Docs"],
      [5, 2, "Bookmarks bar/Docs/Rust"],
      [2, 0, "Other bookmarks"],
      [4, 1, "Other bookmarks/Recipes"],
    ]);
  });

  it("knows what a folder holds", () => {
    const docs = locate(sampleTree(), 3)!.node;
    expect(contains(docs, 6)).toBe(true);
    expect(contains(docs, 3)).toBe(true);
    expect(contains(docs, 8)).toBe(false);
    expect(countLinks(docs)).toBe(2);
    expect(countLinks(locate(sampleTree(), 4)!.node)).toBe(0);
  });

  it("searches titles and URLs for every word, with each match's folder", () => {
    const tree = sampleTree();
    expect(search(tree, "rust BOOK").map(({ link, path }) => [link.id, path.join("/")])).toEqual([
      [6, "Bookmarks bar/Docs/Rust"],
    ]);
    expect(search(tree, "mozilla").map(({ link }) => link.id)).toEqual([7]);
    // Folders are not results; their names are not searched either.
    expect(search(tree, "recipes")).toEqual([]);
    expect(search(tree, "  ")).toEqual([]);
  });

  it("finds the bookmark of a URL added last, as Rust does", () => {
    const tree = sampleTree();
    expect(newestLinkTo(tree, "https://haku.test/")?.id).toBe(8);
    expect(newestLinkTo(tree, "https://haku.test")).toBeNull();

    const twice = sampleTree();
    twice.other = folder(2, "Other bookmarks", [
      { ...link(10, "Later", "https://haku.test/"), addedAt: 5 },
      { ...link(11, "Same time", "https://haku.test/"), addedAt: 5 },
    ]);
    expect(newestLinkTo(twice, "https://haku.test/")?.id).toBe(11);
  });

  it("bookmarks web and file addresses only", () => {
    expect(isBookmarkable("https://a.test/")).toBe(true);
    expect(isBookmarkable("HTTP://a.test")).toBe(true);
    expect(isBookmarkable("file:///C:/notes.html")).toBe(true);
    expect(isBookmarkable("https://")).toBe(false);
    expect(isBookmarkable("haku://settings")).toBe(false);
    expect(isBookmarkable("about:blank")).toBe(false);
  });

  it("counts a later slot among the siblings left once the moved one leaves", () => {
    const siblings = sampleTree().bar.kind;
    const children = "folder" in siblings ? siblings.folder.children : [];
    // Docs (3) at 0, Haku (8) at 1.
    expect(moveIndex(children, 3, 2)).toBe(1);
    expect(moveIndex(children, 8, 0)).toBe(0);
    expect(moveIndex(children, 3, 0)).toBe(0);
    // Arriving from another folder.
    expect(moveIndex(children, 6, 1)).toBe(1);
  });

  it("takes an address typed with no scheme as a web address", () => {
    expect(addressOf(" example.com/a ")).toBe("https://example.com/a");
    expect(addressOf("localhost:3000")).toBe("https://localhost:3000");
    expect(addressOf("http://a.test")).toBe("http://a.test");
    expect(addressOf("file:///C:/notes.html")).toBe("file:///C:/notes.html");
    expect(addressOf("javascript:alert(1)")).toBe("javascript:alert(1)");
    expect(addressOf("")).toBe("");
  });

  it("derives a web address's icon from its host, as tabs do", () => {
    expect(faviconOf("https://doc.rust-lang.org/book/")).toBe("https://doc.rust-lang.org/favicon.ico");
    expect(faviconOf("http://a.test:8080?q")).toBe("http://a.test:8080/favicon.ico");
    expect(faviconOf("file:///C:/notes.html")).toBeNull();
  });
});
