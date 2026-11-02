import type { BookmarkId, BookmarkNode, Bookmarks } from "@bindings";
import { interfaceMenu } from "@features/context-menu/items";
import { feedback } from "@features/feedback/feedback";
import type { MenuItem } from "@features/overlays/menu-model";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";

import { contains, countLinks, displayTitle, isFolder, listFolders, locate, urlOf } from "./tree";

export interface BookmarkMenuActions {
  isPrivate: boolean;
  edit: (id: BookmarkId) => void;
}

/**
 * A bookmark's menu on `haku://bookmarks`: opening a link elsewhere, editing,
 * moving to another folder, and deleting.
 *
 * **Move to** lists every folder by its path, leaving out the one the node is
 * in, and, for a folder, itself and the folders inside it, which Rust refuses.
 */
export function bookmarkMenu(tree: Bookmarks, node: BookmarkNode, actions: BookmarkMenuActions): MenuItem[] {
  const url = urlOf(node);
  const items: MenuItem[] = [];
  if (url !== null) {
    items.push(
      {
        kind: "command",
        id: "openTab",
        label: t("bookmarks.openTab"),
        onSelect: () => void commands.openTab(url, false, "bookmark"),
      },
      {
        kind: "command",
        id: "openWindow",
        label: t("bookmarks.openWindow"),
        onSelect: () => void commands.openWindow(url, null),
      },
    );
    if (!actions.isPrivate) {
      items.push({
        kind: "command",
        id: "openPrivateWindow",
        label: t("bookmarks.openPrivateWindow"),
        onSelect: () => void commands.openWindow(url, true),
      });
    }
    items.push({ kind: "separator", id: "separator:open" });
  }
  const parent = locate(tree, node.id)?.parent?.id;
  const destinations = listFolders(tree).filter(
    ({ folder }) => folder.id !== parent && !(isFolder(node) && contains(node, folder.id)),
  );
  items.push(
    {
      kind: "command",
      id: "edit",
      label: t("bookmarks.edit"),
      onSelect: () => actions.edit(node.id),
    },
    {
      kind: "submenu",
      id: "moveTo",
      label: t("bookmarks.moveTo"),
      disabled: destinations.length === 0,
      items: destinations.map(({ folder, path }) => ({
        kind: "command" as const,
        id: `folder:${folder.id}`,
        label: path.join(" › "),
        onSelect: () => void commands.moveBookmark(node.id, folder.id, null),
      })),
    },
    { kind: "separator", id: "separator:delete" },
    {
      kind: "command",
      id: "delete",
      label: t("bookmarks.delete"),
      onSelect: () => void deleteBookmark(tree, node),
    },
  );
  return interfaceMenu(items);
}

/**
 * Deletes a bookmark. A link or an empty folder goes at once; a folder that
 * holds anything asks first, naming how many bookmarks go with it, since
 * there is no undo.
 */
export async function deleteBookmark(tree: Bookmarks, node: BookmarkNode): Promise<void> {
  if (isFolder(node) && node.kind.folder.children.length > 0) {
    const count = countLinks(node);
    const message =
      count === 0
        ? t("bookmarks.deleteFolder.foldersOnly")
        : count === 1
          ? t("bookmarks.deleteFolder.one")
          : t("bookmarks.deleteFolder.many").replace("{count}", count.toLocaleString());
    const confirmed: unknown = await feedback().dialog.confirm(message, {
      title: t("bookmarks.deleteFolder.title").replace("{name}", () => displayTitle(tree, node)),
      type: "error",
      confirmLabel: t("bookmarks.delete"),
      cancelLabel: t("dialog.cancel"),
    });
    if (confirmed !== true) {
      return;
    }
  }
  await commands.removeBookmark(node.id);
}
