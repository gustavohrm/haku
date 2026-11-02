/**
 * The bookmarks bar's menus, as plain item lists: a folder's contents, the
 * column's **Bookmarks** button, and the row's own menu.
 * [Bookmarks § The bookmarks bar](../../../docs/specs/bookmarks.md#the-bookmarks-bar).
 */

import { BOOKMARKS_URL } from "@app/router";
import type { BookmarkNode, Bookmarks, BookmarksBar, TabId } from "@bindings";
import { interfaceMenu } from "@features/context-menu/items";
import type { MenuItem } from "@features/overlays/menu-model";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";

import { childrenOf, displayTitle, urlOf } from "./tree";

export interface BarActions {
  /** Opens a link in the focused tab. */
  open: (url: string) => void;
  /** Opens links in new background tabs, in order. */
  openAll: (urls: string[]) => void;
  /** Opens `haku://bookmarks` in a new tab. */
  manage: () => void;
}

/** What the bar's menus do, for links opening in `tab`, the focused tab, or a new one when there is none. */
export function barActions(tab: TabId | null): BarActions {
  return {
    open: (url) =>
      void (tab === null ? commands.openTab(url, true, "bookmark") : commands.navigateTab(tab, url, "bookmark")),
    openAll: (urls) => void openInOrder(urls),
    manage: () => void commands.openTab(BOOKMARKS_URL, true, "other"),
  };
}

/** Opens each link in a new background tab, one after another, so the tabs keep the folder's order. */
async function openInOrder(urls: readonly string[]) {
  for (const url of urls) {
    // oxlint-disable-next-line no-await-in-loop -- Each tab opens after the last, in order.
    await commands.openTab(url, false, "bookmark");
  }
}

/**
 * A folder's contents: its links, its folders as submenus, and, when it
 * holds links of its own, **Open all (N)** at the end. An empty folder says
 * so rather than opening nothing.
 */
export function folderItems(tree: Bookmarks, folder: BookmarkNode, actions: BarActions): MenuItem[] {
  const items = nodeItems(tree, childrenOf(folder), actions);
  if (items.length === 0) {
    return [{ kind: "command", id: "empty", label: t("bookmarks.bar.empty"), disabled: true, onSelect: () => {} }];
  }
  const links = childrenOf(folder).flatMap((child) => urlOf(child) ?? []);
  if (links.length > 0) {
    items.push(
      { kind: "separator", id: "separator:openAll" },
      {
        kind: "command",
        id: "openAll",
        label: t("bookmarks.bar.openAll").replace("{count}", links.length.toLocaleString()),
        onSelect: () => actions.openAll(links),
      },
    );
  }
  return items;
}

/**
 * One item per node, as the bar's **»** menu lists what does not fit: a link
 * opens, a folder is a submenu of its own contents.
 */
export function nodeItems(tree: Bookmarks, nodes: readonly BookmarkNode[], actions: BarActions): MenuItem[] {
  return nodes.map((child): MenuItem => {
    const url = urlOf(child);
    if (url === null) {
      return {
        kind: "submenu",
        id: `folder:${child.id}`,
        icon: "ic-folder",
        label: displayTitle(tree, child),
        items: folderItems(tree, child, actions),
      };
    }
    return {
      kind: "command",
      id: `link:${child.id}`,
      label: displayTitle(tree, child),
      onSelect: () => actions.open(url),
    };
  });
}

/**
 * The column's **Bookmarks** button: the bar's contents, then **Other
 * bookmarks** as a submenu and **Manage bookmarks**.
 */
export function bookmarksButtonMenu(tree: Bookmarks, actions: BarActions): MenuItem[] {
  const bar = nodeItems(tree, childrenOf(tree.bar), actions);
  return interfaceMenu([
    ...bar,
    ...(bar.length > 0 ? [{ kind: "separator" as const, id: "separator:other" }] : []),
    {
      kind: "submenu",
      id: "other",
      icon: "ic-folder",
      label: displayTitle(tree, tree.other),
      items: folderItems(tree, tree.other, actions),
    },
    { kind: "command", id: "manage", label: t("bookmarks.bar.manage"), onSelect: actions.manage },
  ]);
}

export interface RowActions {
  /** Opens the form adding a link, or a folder, at the end of the bar. */
  add: (kind: "addLink" | "addFolder") => void;
  setShown: (shown: BookmarksBar) => void;
  manage: () => void;
}

const SHOWN: { value: BookmarksBar; label: "bookmarks.bar.always" | "bookmarks.bar.newTab" | "bookmarks.bar.never" }[] =
  [
    { value: "always", label: "bookmarks.bar.always" },
    { value: "newTab", label: "bookmarks.bar.newTab" },
    { value: "never", label: "bookmarks.bar.never" },
  ];

/**
 * The row's own menu, opened on its empty space: adding to the bar, when it
 * shows, as the setting's three choices, and the bookmarks page.
 */
export function barRowMenu(shown: BookmarksBar, actions: RowActions): MenuItem[] {
  return interfaceMenu([
    { kind: "command", id: "addLink", label: t("bookmarks.bar.addPage"), onSelect: () => actions.add("addLink") },
    { kind: "command", id: "addFolder", label: t("bookmarks.bar.addFolder"), onSelect: () => actions.add("addFolder") },
    { kind: "separator", id: "separator:show" },
    {
      kind: "submenu",
      id: "show",
      label: t("bookmarks.bar.show"),
      items: SHOWN.map(({ value, label }) => ({
        kind: "radio" as const,
        id: value,
        label: t(label),
        checked: value === shown,
        onSelect: () => actions.setShown(value),
      })),
    },
    { kind: "command", id: "manage", label: t("bookmarks.bar.manage"), onSelect: actions.manage },
  ]);
}
