/**
 * Reading the bookmark tree Rust sends: finding nodes, listing folders,
 * searching, and the arithmetic of a move. Kept free of React so it is tested
 * directly. [Bookmarks](../../../docs/specs/bookmarks.md).
 */

import type { BookmarkId, BookmarkNode, Bookmarks } from "@bindings";
import { t } from "@shared/i18n";

export type Folder = BookmarkNode & {
  kind: { folder: { children: BookmarkNode[] } };
};
export type Link = BookmarkNode & { kind: { link: { url: string } } };

export function isFolder(node: BookmarkNode): node is Folder {
  return "folder" in node.kind;
}

export function urlOf(node: BookmarkNode): string | null {
  return "link" in node.kind ? node.kind.link.url : null;
}

export function childrenOf(node: BookmarkNode): BookmarkNode[] {
  return isFolder(node) ? node.kind.folder.children : [];
}

/** Whether `id` is one of the two fixed roots, which cannot be renamed, moved or deleted. */
export function isRoot(tree: Bookmarks, id: BookmarkId): boolean {
  return id === tree.bar.id || id === tree.other.id;
}

/**
 * The name a node is shown by. The roots are named in the interface's
 * language rather than by the English title stored for them; a link with no
 * title is shown by its URL.
 */
export function displayTitle(tree: Bookmarks, node: BookmarkNode): string {
  if (node.id === tree.bar.id) {
    return t("bookmarks.bar");
  }
  if (node.id === tree.other.id) {
    return t("bookmarks.other");
  }
  return node.title || (urlOf(node) ?? "");
}

export interface Located {
  node: BookmarkNode;
  /** The folder holding it, or null for a root. */
  parent: Folder | null;
  /** Its folders from the root down, itself excluded. */
  ancestors: Folder[];
}

/** Finds a node anywhere in the tree. */
export function locate(tree: Bookmarks, id: BookmarkId): Located | null {
  const visit = (node: BookmarkNode, ancestors: Folder[]): Located | null => {
    if (node.id === id) {
      return { node, parent: ancestors.at(-1) ?? null, ancestors };
    }
    if (!isFolder(node)) {
      return null;
    }
    const below = [...ancestors, node];
    for (const child of node.kind.folder.children) {
      const found = visit(child, below);
      if (found) {
        return found;
      }
    }
    return null;
  };
  return visit(tree.bar, []) ?? visit(tree.other, []);
}

export interface FolderEntry {
  folder: Folder;
  depth: number;
  /** Shown names from the root down, the folder's own last. */
  path: string[];
}

/** Every folder, roots first, each followed by the folders inside it. */
export function listFolders(tree: Bookmarks): FolderEntry[] {
  const entries: FolderEntry[] = [];
  const visit = (folder: Folder, depth: number, above: string[]) => {
    const path = [...above, displayTitle(tree, folder)];
    entries.push({ folder, depth, path });
    for (const child of folder.kind.folder.children) {
      if (isFolder(child)) {
        visit(child, depth + 1, path);
      }
    }
  };
  for (const root of [tree.bar, tree.other]) {
    if (isFolder(root)) {
      visit(root, 0, []);
    }
  }
  return entries;
}

/** Whether `id` is `folder` or anything inside it. */
export function contains(folder: BookmarkNode, id: BookmarkId): boolean {
  return folder.id === id || childrenOf(folder).some((child) => contains(child, id));
}

/** How many links a folder holds, at any depth. */
export function countLinks(node: BookmarkNode): number {
  return isFolder(node) ? node.kind.folder.children.reduce((sum, child) => sum + countLinks(child), 0) : 1;
}

export interface Match {
  link: Link;
  /** The shown names of the folders holding it, from the root down. */
  path: string[];
}

/**
 * Every link whose title or URL contains each word of `query`, ignoring
 * case, in tree order.
 */
export function search(tree: Bookmarks, query: string): Match[] {
  const words = query.toLocaleLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) {
    return [];
  }
  const matches: Match[] = [];
  for (const { folder, path } of listFolders(tree)) {
    for (const child of folder.kind.folder.children) {
      const url = urlOf(child);
      if (url === null) {
        continue;
      }
      const haystack = `${child.title}\n${url}`.toLocaleLowerCase();
      if (words.every((word) => haystack.includes(word))) {
        matches.push({ link: child as Link, path });
      }
    }
  }
  return matches;
}

/**
 * The index `move_bookmark` takes to put `moving` where `slot` is in
 * `siblings`, as they are now. Rust counts the index among the siblings
 * after `moving` has left them, so a slot past its old place is one less.
 *
 * @param slot - A position between 0 and `siblings.length`, before the sibling at it.
 */
export function moveIndex(siblings: readonly BookmarkNode[], moving: BookmarkId, slot: number): number {
  const from = siblings.findIndex((sibling) => sibling.id === moving);
  return from !== -1 && from < slot ? slot - 1 : slot;
}

/**
 * What a typed address is stored as. Rust keeps only `http`, `https` and
 * `file` links, so an address typed with no scheme, such as `example.com`,
 * is taken to be a web address. One with another scheme, such as
 * `javascript:`, is kept as typed, for Rust to refuse; a host and port, such
 * as `localhost:3000`, is not a scheme.
 */
export function addressOf(input: string): string {
  const trimmed = input.trim();
  return trimmed === "" || /^[a-z][a-z0-9+.-]*:(?!\d)/i.test(trimmed) ? trimmed : `https://${trimmed}`;
}

/**
 * The icon a tab on `url` would show, as `favicon_for` in `browser.rs`
 * derives it: `/favicon.ico` on the URL's host, or null for anything that is
 * not a web address.
 */
export function faviconOf(url: string): string | null {
  const match = /^(https?):\/\/([^/?#]+)/.exec(url);
  return match ? `${match[1]}://${match[2]}/favicon.ico` : null;
}
