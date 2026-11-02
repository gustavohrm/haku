import { BOOKMARKS_URL } from "@app/router";
import type { BookmarkId } from "@bindings";

/** The folder a `haku://bookmarks?folder=<id>` address opens, or null when it names none. */
export function folderOf(url: string): BookmarkId | null {
  const query = url.startsWith(BOOKMARKS_URL) ? url.slice(BOOKMARKS_URL.length).split("#", 1)[0]! : "";
  const value = new URLSearchParams(query.startsWith("?") ? query : "").get("folder");
  return value !== null && /^\d+$/.test(value) ? Number(value) : null;
}

/** Where a folder opens on the bookmarks page. */
export function folderUrl(folder: BookmarkId): string {
  return `${BOOKMARKS_URL}?folder=${folder}`;
}
