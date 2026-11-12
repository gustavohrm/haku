import { type BookmarkId, type Bookmarks, events, type TabId } from "@bindings";
import { useBarHold } from "@features/bars/holds";
import { interfaceMenu } from "@features/context-menu/items";
import { openMenu } from "@features/context-menu/store";
import type { MenuItem } from "@features/overlays/menu-model";
import { MENU_GAP, useMenuDismiss } from "@features/overlays/use-menu-dismiss";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { useBookmarks, useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useRef, useState, type FormEvent, type RefObject } from "react";
import { createPortal } from "react-dom";

import { folderUrl } from "./bookmarks-url";
import { Favicon } from "./favicon";
import { displayTitle, listFolders, locate, newestLinkTo, urlOf } from "./tree";

interface BookmarkStarProps {
  tab: TabId;
  /** The tab's page, which the caller has checked may be bookmarked. */
  url: string;
}

/** Where the editor opens: its left edge, and its top. */
interface Anchor {
  left: number;
  top: number;
}

/** The editor's width, in pixels, so it can be kept inside the window. The bar's form takes the same. */
export const EDITOR_WIDTH = 320;
/** The least room left between the editor and the window's edges. */
const EDGE = 4;

/**
 * The address bar's star: filled while the page is bookmarked, and the
 * editor it opens on the page's bookmark. Ctrl+D, pressed in Rust, opens the
 * same editor through `BookmarkEditRequested`.
 * [Bookmarks § Bookmarking a page](../../../docs/specs/bookmarks.md#bookmarking-a-page).
 *
 * Mounted once per tab, so switching tabs closes an editor opened for another.
 */
export function BookmarkStar({ tab, url }: BookmarkStarProps) {
  const tree = useBookmarks();
  const [editing, setEditing] = useState<{ id: BookmarkId; anchor: Anchor } | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  // The open editor's close, which keeps an edited name.
  const closeRef = useRef<(() => void) | null>(null);
  const starred = tree !== null && newestLinkTo(tree, url) !== null;

  const edit = useCallback((id: BookmarkId) => {
    const button = buttonRef.current?.getBoundingClientRect();
    if (button) {
      // Under the star, ending where it ends, but inside the window: in the
      // tab column the star is nearer the left edge than the editor is wide.
      const widest = document.documentElement.clientWidth - EDITOR_WIDTH - EDGE;
      const left = Math.max(EDGE, Math.min(button.right - EDITOR_WIDTH, widest));
      setEditing({ id, anchor: { left, top: button.bottom + MENU_GAP } });
    }
  }, []);

  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;
    // This window's own requests, for this tab: one for a tab closed since
    // is dropped, and its bookmark stays.
    const listen = async () => {
      const stop = await events.bookmarkEditRequested(getCurrentWebview()).listen(({ payload }) => {
        if (payload.tab === tab) {
          edit(payload.id);
        }
      });
      if (isMounted) {
        unlisten = stop;
      } else {
        stop();
      }
    };
    void listen();
    return () => {
      isMounted = false;
      unlisten?.();
    };
  }, [tab, edit]);

  const close = useCallback(() => setEditing(null), []);

  const toggle = async () => {
    if (editing) {
      closeRef.current?.();
      return;
    }
    const result = await commands.bookmarkTab(tab);
    if (result.status === "ok") {
      edit(result.data.id);
    }
  };

  const label = starred ? t("bookmarks.star.edit") : t("bookmarks.star.add");
  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="btn icon ghost p-xs -mx-1 shrink-0 [--ui-radius:var(--radius-small)]"
        aria-label={label}
        title={label}
        aria-pressed={starred}
        aria-expanded={editing !== null}
        onClick={() => void toggle()}
      >
        <Star filled={starred} />
      </button>
      {editing && tree && (
        <BookmarkEditor
          key={editing.id}
          tree={tree}
          id={editing.id}
          anchor={editing.anchor}
          buttonRef={buttonRef}
          closeRef={closeRef}
          onClose={close}
        />
      )}
    </>
  );
}

/**
 * Lucide's star, drawn inline so it can be filled: icon classes are masks of
 * the outline alone.
 */
function Star({ filled }: { filled: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      className={cx("size-(--ic-size) [--ic-size:0.875rem]", filled && "text-accent")}
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z" />
    </svg>
  );
}

interface BookmarkEditorProps {
  tree: Bookmarks;
  id: BookmarkId;
  anchor: Anchor;
  buttonRef: RefObject<HTMLButtonElement | null>;
  /** Set to this editor's close, for the star to close it with. */
  closeRef: RefObject<(() => void) | null>;
  onClose: () => void;
}

/**
 * The popover editing one bookmark: its name, its folder, **Remove** and
 * **Done**. A folder chosen moves it at once; the name is kept when the
 * editor closes, however it closes, and Remove alone discards it. The folder
 * menu ends with **New folder…**, which names a folder in place, and
 * **Manage bookmarks**.
 */
function BookmarkEditor({ tree, id, anchor, buttonRef, closeRef, onClose }: BookmarkEditorProps) {
  const { private: isPrivate } = useBrowserState();
  const located = locate(tree, id);
  const [name, setName] = useState(located?.node.title ?? "");
  const nameRef = useRef(name);
  useEffect(() => {
    nameRef.current = name;
  }, [name]);
  // A new folder's name while one is being named, in place of the folder.
  const [folderName, setFolderName] = useState<string | null>(null);
  const savedRef = useRef(located?.node.title ?? "");
  const removedRef = useRef(false);
  // While the folder menu is open, a click in it is not a click away.
  const choosingRef = useRef(false);
  const editorRef = useRef<HTMLElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  // Set when the new folder's field closes, so the folder button that takes
  // its place takes its focus too.
  const refocusFolder = useRef(false);

  useOverlay(editorRef, true);
  useBarHold("toolbar", true);

  const close = useCallback(() => {
    if (choosingRef.current) {
      return;
    }
    if (!removedRef.current && nameRef.current !== savedRef.current) {
      savedRef.current = nameRef.current;
      void commands.updateBookmark(id, nameRef.current, null);
    }
    onClose();
  }, [id, onClose]);
  useMenuDismiss(true, close, editorRef, buttonRef);
  useEffect(() => {
    closeRef.current = close;
    return () => {
      closeRef.current = null;
    };
  }, [close, closeRef]);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  // Removed from elsewhere, there is nothing left to edit.
  const gone = located === null;
  useEffect(() => {
    if (gone) {
      removedRef.current = true;
      onClose();
    }
  }, [gone, onClose]);
  if (!located?.parent) {
    return null;
  }
  const parent = located.parent;
  const url = urlOf(located.node);
  const folderTitle = displayTitle(tree, parent);

  const chooseFolder = (button: HTMLElement) => {
    const box = button.getBoundingClientRect();
    choosingRef.current = true;
    const folders: MenuItem[] = listFolders(tree).map(({ folder, path }) => ({
      kind: "radio",
      id: `folder:${folder.id}`,
      label: path.join(" › "),
      checked: folder.id === parent.id,
      onSelect: () => void commands.moveBookmark(id, folder.id, null),
    }));
    openMenu({
      at: { x: box.left, y: box.bottom + MENU_GAP },
      label: t("bookmarks.folder"),
      items: interfaceMenu([
        ...folders,
        { kind: "separator", id: "separator:newFolder" },
        {
          kind: "command",
          id: "newFolder",
          label: t("bookmarks.editor.newFolder"),
          icon: "ic-folder-plus",
          onSelect: () => setFolderName(""),
        },
        {
          kind: "command",
          id: "manage",
          label: t("bookmarks.bar.manage"),
          icon: "ic-bookmark",
          onSelect: () => {
            // Chosen from the menu, so not a click away: close it here.
            choosingRef.current = false;
            close();
            void commands.openTab(folderUrl(parent.id), true, "other");
          },
        },
      ]),
      onClose: () => {
        choosingRef.current = false;
      },
    });
  };

  const closeFolderName = () => {
    refocusFolder.current = true;
    setFolderName(null);
  };

  // A refused folder keeps its field, with the name typed, to try again.
  const createFolder = async () => {
    const title = folderName?.trim() || t("bookmarks.newFolder");
    const created = await commands.addBookmarkFolder(parent.id, null, title);
    if (created.status === "ok") {
      closeFolderName();
      await commands.moveBookmark(id, created.data, null);
    }
  };

  const done = (event: FormEvent) => {
    event.preventDefault();
    close();
  };

  return createPortal(
    // On the overlay surface, opaque, and without a shadow, which the input
    // mask would cut off at its edge. No heading: the star that opened it
    // already says what it edits.
    <section
      ref={editorRef}
      aria-label={t("bookmarks.editor.title")}
      tabIndex={-1}
      className="haku-overlay fixed z-10 flex flex-col p-2 outline-none"
      style={{ left: anchor.left, top: anchor.top, width: EDITOR_WIDTH }}
    >
      <form className="flex flex-col gap-2" onSubmit={done}>
        <span className="input-group soft edgeless unadorned">
          {/* The generic icon in a private window: fetching the site's would send the ordinary profile's cookies. */}
          <Favicon key={url ?? ""} url={isPrivate ? null : url} small />
          <input
            ref={inputRef}
            className="ipt"
            aria-label={t("bookmarks.name")}
            value={name}
            onChange={(event) => setName(event.target.value)}
            spellCheck={false}
            autoComplete="off"
          />
        </span>
        {folderName === null ? (
          <button
            ref={(button) => {
              if (button && refocusFolder.current) {
                refocusFolder.current = false;
                button.focus();
              }
            }}
            type="button"
            className="btn soft edgeless w-full justify-start gap-2 px-3 font-normal"
            aria-haspopup="menu"
            aria-label={`${t("bookmarks.folder")}: ${folderTitle}`}
            onClick={(event) => chooseFolder(event.currentTarget)}
          >
            <i className="ic-folder ic-sm text-text-secondary shrink-0" aria-hidden="true" />
            <span className="min-w-0 flex-1 truncate text-left">{folderTitle}</span>
            <i className="ic-chevron-down ic-sm text-text-secondary shrink-0" aria-hidden="true" />
          </button>
        ) : (
          <span className="input-group soft edgeless unadorned">
            <i className="ic-folder-plus ic-sm text-text-secondary shrink-0" aria-hidden="true" />
            <input
              className="ipt"
              aria-label={t("bookmarks.editor.folderName")}
              placeholder={t("bookmarks.newFolder")}
              value={folderName}
              spellCheck={false}
              autoComplete="off"
              // oxlint-disable-next-line jsx-a11y/no-autofocus -- It replaces the folder button the user just asked to name a folder from.
              autoFocus
              onChange={(event) => setFolderName(event.target.value)}
              onKeyDown={(event) => {
                // Its own Enter and Esc: the editor's would close the editor.
                if (event.key === "Enter") {
                  event.preventDefault();
                  void createFolder();
                } else if (event.key === "Escape") {
                  event.stopPropagation();
                  closeFolderName();
                }
              }}
            />
          </span>
        )}
        <div className="mt-1 flex items-center gap-2">
          <button
            type="button"
            className="btn ghost text-text-secondary"
            onClick={() => {
              removedRef.current = true;
              onClose();
              void commands.removeBookmark(id);
            }}
          >
            {t("bookmarks.remove")}
          </button>
          <button type="submit" className="btn primary ml-auto">
            {t("bookmarks.done")}
          </button>
        </div>
      </form>
    </section>,
    document.body,
  );
}
