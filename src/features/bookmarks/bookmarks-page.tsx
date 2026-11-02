import { InternalPage } from "@app/internal-page";
import type { InternalPageProps } from "@app/router";
import type { BookmarkId, BookmarkNode, Bookmarks } from "@bindings";
import { openMenu } from "@features/context-menu/store";
import { MENU_GAP } from "@features/overlays/use-menu-dismiss";
import { commands } from "@ipc/commands";
import { useBookmarks, useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useState, type MouseEvent, type ReactNode } from "react";

import { BookmarkForm, type FormTarget } from "./bookmark-form";
import { bookmarkMenu } from "./bookmark-menu";
import { folderOf, folderUrl } from "./bookmarks-url";
import { Favicon } from "./favicon";
import { exportBookmarks, importBookmarks } from "./transfer";
import { childrenOf, displayTitle, isFolder, isRoot, locate, search, urlOf, type Folder } from "./tree";
import { dropEdge, isDropInto, useBookmarkDrag } from "./use-bookmark-drag";

/**
 * `haku://bookmarks`: the folder tree beside the open folder's contents, a
 * search over every bookmark, and adding, editing, moving and deleting.
 * `?folder=<id>` opens a folder. [Bookmarks § The bookmarks page](../../../docs/specs/bookmarks.md#the-bookmarks-page).
 */
export function BookmarksPage({ tabId, url }: InternalPageProps) {
  const tree = useBookmarks();
  const { private: isPrivate } = useBrowserState();
  const [query, setQuery] = useState("");
  const [form, setForm] = useState<FormTarget | null>(null);
  const drag = useBookmarkDrag(tree);

  if (!tree) {
    return <InternalPage title={t("bookmarks.title")}>{null}</InternalPage>;
  }

  const open = openFolder(tree, folderOf(url));
  const searching = query.trim() !== "";
  const go = (folder: BookmarkId) => {
    setQuery("");
    setForm(null);
    if (folder !== open.id) {
      void commands.navigateTab(tabId, folderUrl(folder));
    }
  };
  const showMenu = (node: BookmarkNode, at: { x: number; y: number }) =>
    openMenu({
      at,
      label: displayTitle(tree, node),
      items: bookmarkMenu(tree, node, { isPrivate, edit: (id) => setForm({ kind: "edit", id }) }),
    });
  const onContextMenu = (node: BookmarkNode) => (event: MouseEvent) => {
    event.preventDefault();
    showMenu(node, { x: event.clientX, y: event.clientY });
  };
  const follow = (link: string, event: MouseEvent) => {
    if (event.button === 1 || event.ctrlKey || event.metaKey) {
      void commands.openTab(link, false);
    } else if (event.shiftKey) {
      void commands.openWindow(link, null);
    } else {
      void commands.navigateTab(tabId, link);
    }
  };

  const row = (node: BookmarkNode, slot: number, parent: BookmarkId, detail: ReactNode) => {
    const link = urlOf(node);
    if (form?.kind === "edit" && form.id === node.id) {
      return (
        <li key={node.id}>
          <BookmarkForm tree={tree} target={form} onDone={() => setForm(null)} />
        </li>
      );
    }
    const edge = searching ? null : dropEdge(drag.target, node.id);
    const title = displayTitle(tree, node);
    return (
      // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Right-click opens the row's menu; its actions button opens the same from the keyboard.
      <li
        key={node.id}
        className={cx(
          "group relative flex items-center gap-1 rounded-(--radius-control)",
          isDropInto(drag.target, node.id) && "bg-text/10",
          drag.dragging === node.id && "opacity-50",
          edge === "before" && "before:bg-primary before:absolute before:inset-x-0 before:-top-px before:h-0.5",
          edge === "after" && "after:bg-primary after:absolute after:inset-x-0 after:-bottom-px after:h-0.5",
        )}
        {...(searching
          ? {}
          : {
              "data-drop-row": node.id,
              "data-drop-parent": parent,
              "data-drop-slot": slot,
              ...(isFolder(node) ? { "data-drop-folder": node.id } : {}),
            })}
        onContextMenu={onContextMenu(node)}
      >
        <button
          type="button"
          className="hover:bg-text/6 flex min-w-0 flex-1 touch-none items-center gap-3 rounded-(--radius-control) px-3 py-2 text-left select-none"
          onPointerDown={searching ? undefined : (event) => drag.start(node.id, event)}
          onClick={(event) => {
            if (drag.consumeClick()) {
              return;
            }
            if (link === null) {
              go(node.id);
            } else {
              follow(link, event);
            }
          }}
          onAuxClick={(event) => {
            if (event.button === 1 && link !== null) {
              event.preventDefault();
              follow(link, event);
            }
          }}
        >
          {link === null ? (
            <i className="ic-folder ic-md text-text-secondary shrink-0" aria-hidden="true" />
          ) : (
            <Favicon key={link} url={isPrivate ? null : link} />
          )}
          <span className="min-w-0 flex-1 truncate font-medium">{title}</span>
          <span className="text-text-secondary max-w-1/2 truncate">{detail}</span>
        </button>
        <button
          type="button"
          className="btn icon ghost shrink-0 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"
          aria-label={t("bookmarks.actions").replace("{name}", () => title)}
          aria-haspopup="menu"
          title={t("bookmarks.actions").replace("{name}", () => title)}
          onClick={(event) => {
            const box = event.currentTarget.getBoundingClientRect();
            showMenu(node, { x: box.left, y: box.bottom + MENU_GAP });
          }}
        >
          <i className="ic-ellipsis ic-sm" aria-hidden="true" />
        </button>
      </li>
    );
  };

  const sidebar = (
    <>
      <label className="input-group soft edgeless unadorned">
        <i className="ic-search ic-sm text-text-secondary" aria-hidden="true" />
        <input
          type="search"
          className="ipt"
          placeholder={t("bookmarks.search")}
          aria-label={t("bookmarks.search")}
          value={query}
          spellCheck={false}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              setQuery("");
            }
          }}
        />
      </label>
      <nav aria-label={t("bookmarks.folders")}>
        <FolderTree
          tree={tree}
          open={searching ? null : open.id}
          dropInto={(id) => isDropInto(drag.target, id)}
          onOpen={go}
          onContextMenu={onContextMenu}
        />
      </nav>
    </>
  );

  if (searching) {
    const matches = search(tree, query);
    return (
      <InternalPage title={t("bookmarks.searchResults")} sidebar={sidebar}>
        {matches.length === 0 ? (
          <p className="text-text-secondary">{t("bookmarks.searchEmpty")}</p>
        ) : (
          <ul className="flex flex-col">
            {matches.map(({ link, path }, index) => row(link, index, open.id, path.join(" › ")))}
          </ul>
        )}
      </InternalPage>
    );
  }

  const children = childrenOf(open);
  return (
    <InternalPage
      title={displayTitle(tree, open)}
      sidebar={sidebar}
      actions={
        <div className="flex gap-2">
          <button
            type="button"
            className="btn ghost edged gap-2"
            onClick={() => setForm({ kind: "addLink", parent: open.id })}
          >
            <i className="ic-plus ic-sm" aria-hidden="true" />
            {t("bookmarks.addPage")}
          </button>
          <button
            type="button"
            className="btn ghost edged gap-2"
            onClick={() => setForm({ kind: "addFolder", parent: open.id })}
          >
            <i className="ic-folder-plus ic-sm" aria-hidden="true" />
            {t("bookmarks.addFolder")}
          </button>
          <button type="button" className="btn ghost edged gap-2" onClick={() => void importBookmarks()}>
            <i className="ic-upload ic-sm" aria-hidden="true" />
            {t("bookmarks.import")}
          </button>
          <button type="button" className="btn ghost edged gap-2" onClick={() => void exportBookmarks()}>
            <i className="ic-download ic-sm" aria-hidden="true" />
            {t("bookmarks.export")}
          </button>
        </div>
      }
    >
      {form && form.kind !== "edit" && <BookmarkForm tree={tree} target={form} onDone={() => setForm(null)} />}
      {children.length === 0 ? (
        !form && <p className="text-text-secondary">{t("bookmarks.empty")}</p>
      ) : (
        <ul className="flex flex-col">
          {children.map((child, index) => row(child, index, open.id, urlOf(child) ?? ""))}
        </ul>
      )}
    </InternalPage>
  );
}

/** The folder `?folder=` names, or the bar when it names none or one that is gone. */
function openFolder(tree: Bookmarks, id: BookmarkId | null): Folder {
  const node = id === null ? null : locate(tree, id)?.node;
  return node && isFolder(node) ? node : (tree.bar as Folder);
}

interface FolderTreeProps {
  tree: Bookmarks;
  open: BookmarkId | null;
  dropInto: (id: BookmarkId) => boolean;
  onOpen: (id: BookmarkId) => void;
  onContextMenu: (node: BookmarkNode) => (event: MouseEvent) => void;
}

/**
 * Every folder, nested. A folder's subfolders show while it is expanded, and
 * always while the open folder is inside it, so the open one is never hidden.
 */
function FolderTree({ tree, open, dropInto, onOpen, onContextMenu }: FolderTreeProps) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<BookmarkId>>(() => new Set());
  const [expanded, setExpanded] = useState<ReadonlySet<BookmarkId>>(() => new Set());
  const openPath = new Set(open === null ? [] : (locate(tree, open)?.ancestors.map((folder) => folder.id) ?? []));

  const toggle = (id: BookmarkId, shown: boolean) => {
    const without = (set: ReadonlySet<BookmarkId>) => new Set([...set].filter((entry) => entry !== id));
    if (shown) {
      setExpanded(without);
      setCollapsed((set) => new Set([...set, id]));
    } else {
      setCollapsed(without);
      setExpanded((set) => new Set([...set, id]));
    }
  };

  const branch = (folder: Folder, depth: number): ReactNode => {
    const subfolders = folder.kind.folder.children.filter(isFolder);
    // The roots start open; anything else starts closed.
    const shown =
      subfolders.length > 0 &&
      (openPath.has(folder.id) || expanded.has(folder.id) || (depth === 0 && !collapsed.has(folder.id)));
    const active = folder.id === open;
    return (
      <li key={folder.id}>
        <div
          data-drop-folder={folder.id}
          className={cx(
            "flex h-9 items-center rounded-(--radius-control) transition-colors",
            active ? "bg-surface text-text" : "text-text-secondary hover:bg-text/6 hover:text-text",
            dropInto(folder.id) && "bg-text/10",
          )}
          style={{ paddingInlineStart: `${depth * 0.75}rem` }}
          onContextMenu={isRoot(tree, folder.id) ? undefined : onContextMenu(folder)}
        >
          {subfolders.length > 0 ? (
            <button
              type="button"
              className="btn icon ghost p-xs shrink-0 [--ui-radius:var(--radius-small)]"
              aria-label={displayTitle(tree, folder)}
              aria-expanded={shown}
              onClick={() => toggle(folder.id, shown)}
            >
              <i className={cx(shown ? "ic-chevron-down" : "ic-chevron-right", "ic-sm")} aria-hidden="true" />
            </button>
          ) : (
            <span className="w-6 shrink-0" />
          )}
          <button
            type="button"
            className="flex h-full min-w-0 flex-1 items-center gap-2 pe-2.5 text-left"
            aria-current={active ? "page" : undefined}
            onClick={() => onOpen(folder.id)}
          >
            <i className={cx(active ? "ic-folder-open" : "ic-folder", "ic-md shrink-0")} aria-hidden="true" />
            <span className="truncate">{displayTitle(tree, folder)}</span>
          </button>
        </div>
        {shown && <ul>{subfolders.map((child) => branch(child, depth + 1))}</ul>}
      </li>
    );
  };

  return (
    <ul className="flex flex-col gap-0.5">{[tree.bar, tree.other].filter(isFolder).map((root) => branch(root, 0))}</ul>
  );
}
