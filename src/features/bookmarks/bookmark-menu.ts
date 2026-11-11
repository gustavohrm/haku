import type { BookmarkId, BookmarkNode, Bookmarks } from "@bindings";
import { interfaceMenu } from "@features/context-menu/items";
import type { MenuItem } from "@features/overlays/menu-model";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";

import { contains, countLinks, displayTitle, isFolder, listFolders, locate, urlOf } from "./tree";

export interface BookmarkMenuActions {
  isPrivate: boolean;
  edit: (id: BookmarkId) => void;
  /**
   * Asks on the bookmark's row before a folder that holds anything is
   * removed, where the menu was opened from a row. Without it, the menu asks
   * in its own place.
   */
  confirmInRow?: (id: BookmarkId) => void;
}

/**
 * A bookmark's menu, on `haku://bookmarks` and the bookmarks bar: opening a
 * link elsewhere, editing, moving to another folder, and removing.
 *
 * **Move to** lists every folder by its path, leaving out the one the node is
 * in, and, for a folder, itself and the folders inside it, which Rust refuses.
 *
 * **Remove** takes a link or an empty folder at once. A folder that holds
 * anything asks first, naming how many bookmarks go with it, since there is
 * no undo: on its row when there is one, or where the menu was.
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
  const question = removeQuestion(tree, node);
  const { confirmInRow } = actions;
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
    { kind: "separator", id: "separator:remove" },
    question !== null && confirmInRow
      ? { kind: "command", id: "remove", label: t("bookmarks.remove"), onSelect: () => confirmInRow(node.id) }
      : {
          kind: "command",
          id: "remove",
          label: t("bookmarks.remove"),
          ...(question === null
            ? {}
            : { confirm: { message: question, action: t("bookmarks.remove"), destructive: true } }),
          onSelect: () => void commands.removeBookmark(node.id),
        },
  );
  return interfaceMenu(items);
}

/**
 * What removing a bookmark asks first: nothing for a link or an empty folder,
 * and for a folder that holds anything, how many bookmarks go with it.
 */
export function removeQuestion(tree: Bookmarks, node: BookmarkNode): string | null {
  if (!isFolder(node) || node.kind.folder.children.length === 0) {
    return null;
  }
  const count = countLinks(node);
  const question =
    count === 0
      ? t("bookmarks.removeFolder.foldersOnly")
      : count === 1
        ? t("bookmarks.removeFolder.one")
        : t("bookmarks.removeFolder.many").replace("{count}", count.toLocaleString());
  return question.replace("{name}", () => displayTitle(tree, node));
}
