import type { BookmarkNode, Bookmarks } from "@bindings";

export function link(id: number, title: string, url: string): BookmarkNode {
  return { id, title, addedAt: 0, modifiedAt: 0, kind: { link: { url } } };
}

export function folder(id: number, title: string, children: BookmarkNode[]): BookmarkNode {
  return { id, title, addedAt: 0, modifiedAt: 0, kind: { folder: { children } } };
}

/**
 * Bookmarks bar (1): Docs (3) [Rust (5) [The Book (6)], MDN (7)], Haku (8)
 * Other bookmarks (2): Recipes (4) []
 */
export function sampleTree(): Bookmarks {
  return {
    bar: folder(1, "Bookmarks bar", [
      folder(3, "Docs", [
        folder(5, "Rust", [link(6, "The Book", "https://doc.rust-lang.org/book/")]),
        link(7, "MDN", "https://developer.mozilla.org/"),
      ]),
      link(8, "Haku", "https://haku.test/"),
    ]),
    other: folder(2, "Other bookmarks", [folder(4, "Recipes", [])]),
  };
}
