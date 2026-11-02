import { type BookmarkId, type Bookmarks, events, type TabId } from "@bindings";
import { useBarHold } from "@features/bars/holds";
import { interfaceMenu } from "@features/context-menu/items";
import { openMenu } from "@features/context-menu/store";
import { MENU_GAP, useMenuDismiss } from "@features/overlays/use-menu-dismiss";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { useBookmarks } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useId, useRef, useState, type FormEvent, type RefObject } from "react";
import { createPortal } from "react-dom";

import { folderUrl } from "./bookmarks-url";
import { displayTitle, listFolders, locate, newestLinkTo } from "./tree";

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

/** The editor's width, in pixels, so it can be kept inside the window. */
const EDITOR_WIDTH = 320;
/** The least room left between the editor and the window's edges. */
const EDGE = 4;

/**
 * The address field's star: filled while the page is bookmarked, and the
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
 * The popover editing one bookmark: its name, its folder, **Remove**,
 * **More…** and **Done**. A folder chosen moves it at once; the name is kept
 * when the editor closes, however it closes, and Remove alone discards it.
 */
function BookmarkEditor({ tree, id, anchor, buttonRef, closeRef, onClose }: BookmarkEditorProps) {
  const located = locate(tree, id);
  const [name, setName] = useState(located?.node.title ?? "");
  const nameRef = useRef(name);
  useEffect(() => {
    nameRef.current = name;
  }, [name]);
  const savedRef = useRef(located?.node.title ?? "");
  const removedRef = useRef(false);
  // While the folder menu is open, a click in it is not a click away.
  const choosingRef = useRef(false);
  const editorRef = useRef<HTMLElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const titleId = useId();
  const nameId = useId();

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

  const chooseFolder = (button: HTMLElement) => {
    const box = button.getBoundingClientRect();
    choosingRef.current = true;
    openMenu({
      at: { x: box.left, y: box.bottom + MENU_GAP },
      label: t("bookmarks.folder"),
      items: interfaceMenu(
        listFolders(tree).map(({ folder, path }) => ({
          kind: "radio" as const,
          id: `folder:${folder.id}`,
          label: path.join(" › "),
          checked: folder.id === parent.id,
          onSelect: () => void commands.moveBookmark(id, folder.id, null),
        })),
      ),
      onClose: () => {
        choosingRef.current = false;
      },
    });
  };

  const done = (event: FormEvent) => {
    event.preventDefault();
    close();
  };

  return createPortal(
    // Opaque and without a shadow, because the input mask cannot show
    // either over the page.
    <section
      ref={editorRef}
      aria-labelledby={titleId}
      tabIndex={-1}
      className="bg-chrome-raised border-text/15 text-text fixed z-10 flex flex-col gap-3 rounded-(--radius-surface) border p-3 outline-none"
      style={{ left: anchor.left, top: anchor.top, width: EDITOR_WIDTH }}
    >
      <h2 id={titleId} className="text-sm font-semibold">
        {t("bookmarks.editor.title")}
      </h2>
      <form className="flex flex-col gap-3" onSubmit={done}>
        <label htmlFor={nameId} className="flex flex-col gap-1 text-xs">
          {t("bookmarks.name")}
          <span className="input-group soft edgeless unadorned">
            <input
              ref={inputRef}
              id={nameId}
              className="ipt"
              value={name}
              onChange={(event) => setName(event.target.value)}
              spellCheck={false}
              autoComplete="off"
            />
          </span>
        </label>
        <div className="flex flex-col gap-1 text-xs">
          {t("bookmarks.folder")}
          <button
            type="button"
            className="btn soft edgeless justify-between"
            aria-haspopup="menu"
            onClick={(event) => chooseFolder(event.currentTarget)}
          >
            <span className="truncate">{displayTitle(tree, parent)}</span>
            <i className="ic-chevron-down ic-sm shrink-0" aria-hidden="true" />
          </button>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="btn ghost edged"
            onClick={() => {
              removedRef.current = true;
              onClose();
              void commands.removeBookmark(id);
            }}
          >
            {t("bookmarks.remove")}
          </button>
          <button
            type="button"
            className="btn ghost edged"
            onClick={() => {
              close();
              void commands.openTab(folderUrl(parent.id), true);
            }}
          >
            {t("bookmarks.more")}
          </button>
          <button type="submit" className="btn ml-auto">
            {t("bookmarks.done")}
          </button>
        </div>
      </form>
    </section>,
    document.body,
  );
}
