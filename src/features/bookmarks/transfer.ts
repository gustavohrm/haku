/**
 * **Import…** and **Export…** on `haku://bookmarks`. Rust opens the file
 * dialogs and reads or writes the file; the interface only says what came of
 * it. A failure is reported by the command.
 * [Bookmarks § Import and export](../../../docs/specs/bookmarks.md#import-and-export).
 */
import { feedback } from "@features/feedback/feedback";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";

type Counted = "links" | "folders" | "skipped";

/** "1 link", "82 links", and so on. */
function counted(kind: Counted, count: number): string {
  return count === 1
    ? t(`bookmarks.count.${kind}.one`)
    : t(`bookmarks.count.${kind}.many`).replace("{count}", count.toLocaleString());
}

/** "82 links and 33 folders". */
function linksAndFolders(links: number, folders: number): string {
  return t("bookmarks.count.and")
    .replace("{links}", counted("links", links))
    .replace("{folders}", counted("folders", folders));
}

/** Asks for a bookmark file, imports it, and says how much it added and skipped. */
export async function importBookmarks(): Promise<void> {
  const result = await commands.importBookmarks();
  if (result.status === "error" || result.data === null) {
    return;
  }
  const { links, folders, skipped } = result.data;
  const parts = [links + folders === 0 ? t("bookmarks.imported.nothing") : linksAndFolders(links, folders)];
  if (skipped > 0) {
    parts.push(counted("skipped", skipped));
  }
  feedback().success({ title: t("bookmarks.imported"), description: parts.join(" ") });
}

/** Asks where to export, writes every bookmark there, and says how many. */
export async function exportBookmarks(): Promise<void> {
  const result = await commands.exportBookmarks();
  if (result.status === "error" || result.data === null) {
    return;
  }
  feedback().success({
    title: t("bookmarks.exported"),
    description: linksAndFolders(result.data.links, result.data.folders),
  });
}
