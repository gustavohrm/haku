import type { BookmarkId, BookmarkNode, Bookmarks, TabId } from "@bindings";
import { type BarContent, holdBar, useBarHold } from "@features/bars/holds";
import { openMenu } from "@features/context-menu/store";
import { MENU_GAP, useMenuDismiss } from "@features/overlays/use-menu-dismiss";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { useBookmarks, useBrowserState, useSettings } from "@ipc/hooks";
import { updateSettings } from "@ipc/store";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { memo, useCallback, useLayoutEffect, useRef, useState, type MouseEvent, type PointerEvent } from "react";
import { createPortal } from "react-dom";

import { barActions, barRowMenu, folderItems, nodeItems } from "./bar-menus";
import { BookmarkForm, type FormTarget } from "./bookmark-form";
import { bookmarkMenu } from "./bookmark-menu";
import { Favicon } from "./favicon";
import { childrenOf, displayTitle, isFolder, urlOf } from "./tree";

interface BookmarksBarProps {
  /** The focused tab, where a link opens. */
  tab: TabId | null;
  /** The floating bar this row is in, which its menus and forms keep shown. */
  hold: BarContent;
  /** A row of its own, at the top of the page area, rather than under the toolbar. */
  standalone?: boolean;
}

/** A point in the window's CSS pixels. */
interface Point {
  x: number;
  y: number;
}

const NO_CHILDREN: readonly BookmarkNode[] = [];

/** What a click on a link asks for, from the button and keys held. */
type Follow = "here" | "background" | "window";

function followOf(event: MouseEvent): Follow {
  if (event.button === 1 || event.ctrlKey || event.metaKey) {
    return "background";
  }
  return event.shiftKey ? "window" : "here";
}

/**
 * The bookmarks bar: the bar root's links and folders in a row, with what does
 * not fit in a **»** menu at its end.
 * [Bookmarks § The bookmarks bar](../../../docs/specs/bookmarks.md#the-bookmarks-bar).
 *
 * Every item is drawn and measured, and those past the row's end are hidden
 * and listed in the **»** menu instead, so the row needs no guess at how wide
 * a title is.
 */
export function BookmarksBar({ tab, hold, standalone = false }: BookmarksBarProps) {
  const tree = useBookmarks();
  const settings = useSettings();
  const { private: isPrivate } = useBrowserState();
  const listRef = useRef<HTMLUListElement>(null);
  const [fits, setFits] = useState(Number.POSITIVE_INFINITY);
  const [form, setForm] = useState<{ target: FormTarget; at: Point } | null>(null);
  const [expanded, setExpanded] = useState<BookmarkId | "overflow" | null>(null);

  const shown = settings?.bookmarksBar ?? "newTab";

  // Read when an item is used rather than captured, so the items' handlers
  // stay the same across changes and an unchanged item is not drawn again.
  const latest = useRef({ tree, tab, isPrivate });
  useLayoutEffect(() => {
    latest.current = { tree, tab, isPrivate };
  });

  const children = tree ? childrenOf(tree.bar) : NO_CHILDREN;
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list) {
      return;
    }
    const measure = () => {
      const limit = list.clientWidth;
      let count = 0;
      for (const item of list.children) {
        const element = item as HTMLElement;
        if (element.offsetLeft + element.offsetWidth > limit) {
          break;
        }
        count += 1;
      }
      setFits(count);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(list);
    return () => observer.disconnect();
  }, [children]);

  const follow = useCallback((url: string, how: Follow) => {
    if (how === "background") {
      void commands.openTab(url, false, "bookmark");
    } else if (how === "window") {
      void commands.openWindow(url, null);
    } else {
      barActions(latest.current.tab).open(url);
    }
  }, []);

  const openFolder = useCallback(
    (node: BookmarkNode, button: HTMLElement) => {
      const current = latest.current.tree;
      if (!current) {
        return;
      }
      const box = button.getBoundingClientRect();
      setExpanded(node.id);
      const release = holdBar(hold);
      openMenu({
        at: { x: box.left, y: box.bottom + MENU_GAP },
        label: displayTitle(current, node),
        items: folderItems(current, node, barActions(latest.current.tab)),
        onClose: () => {
          release();
          setExpanded((open) => (open === node.id ? null : open));
        },
      });
    },
    [hold],
  );

  const showItemMenu = useCallback(
    (node: BookmarkNode, at: Point) => {
      const current = latest.current.tree;
      if (!current) {
        return;
      }
      openMenu({
        at,
        label: displayTitle(current, node),
        items: bookmarkMenu(current, node, {
          isPrivate: latest.current.isPrivate,
          edit: (id) => setForm({ target: { kind: "edit", id }, at }),
        }),
        onClose: holdBar(hold),
      });
    },
    [hold],
  );

  if (!tree) {
    return null;
  }

  const showRowMenu = (event: MouseEvent) => {
    event.preventDefault();
    const at = { x: event.clientX, y: event.clientY };
    openMenu({
      at,
      label: t("bookmarks.bar"),
      items: barRowMenu(shown, {
        add: (kind) => setForm({ target: { kind, parent: tree.bar.id }, at }),
        setShown: (bookmarksBar) => void updateSettings((current) => ({ ...current, bookmarksBar })),
        manage: barActions(tab).manage,
      }),
      onClose: holdBar(hold),
    });
  };

  const overflow = children.slice(fits);
  const openOverflow = (button: HTMLElement) => {
    const box = button.getBoundingClientRect();
    setExpanded("overflow");
    const release = holdBar(hold);
    openMenu({
      at: { x: box.left, y: box.bottom + MENU_GAP },
      label: t("bookmarks.bar.more"),
      items: nodeItems(tree, overflow, barActions(tab)),
      onClose: () => {
        release();
        setExpanded((open) => (open === "overflow" ? null : open));
      },
    });
  };

  return (
    <>
      <div
        data-bookmarks-bar
        className={cx(
          "haku-bar flex h-[calc(1.5rem+var(--haku-gap))] shrink-0 items-start gap-0.5 px-(--haku-gap)",
          standalone && "box-content pt-(--haku-gap)",
        )}
        onContextMenu={showRowMenu}
      >
        <ul
          ref={listRef}
          className="relative flex min-w-0 flex-1 gap-0.5 overflow-hidden"
          aria-label={t("bookmarks.bar")}
        >
          {children.map((node, index) => (
            <BarItem
              key={node.id}
              node={node}
              title={displayTitle(tree, node)}
              isPrivate={isPrivate}
              isHidden={index >= fits}
              isExpanded={expanded === node.id}
              onFollow={follow}
              onOpenFolder={openFolder}
              onMenu={showItemMenu}
            />
          ))}
        </ul>
        {overflow.length > 0 && (
          <button
            type="button"
            className="btn ghost h-6 shrink-0 px-1.5 text-xs"
            aria-label={t("bookmarks.bar.more")}
            title={t("bookmarks.bar.more")}
            aria-haspopup="menu"
            aria-expanded={expanded === "overflow"}
            onClick={(event) => openOverflow(event.currentTarget)}
          >
            »
          </button>
        )}
      </div>
      {/* Beside the row rather than in it: React passes a portal's events up its
        own tree, so a right-click in a field would open the row's menu. */}
      {form && <BarForm tree={tree} target={form.target} at={form.at} hold={hold} onDone={() => setForm(null)} />}
    </>
  );
}

interface BarItemProps {
  node: BookmarkNode;
  title: string;
  isPrivate: boolean;
  /** Past the row's end: listed in the » menu, and out of reach here. */
  isHidden: boolean;
  isExpanded: boolean;
  onFollow: (url: string, how: Follow) => void;
  onOpenFolder: (node: BookmarkNode, button: HTMLElement) => void;
  onMenu: (node: BookmarkNode, at: Point) => void;
}

/**
 * Whether an item would draw the same: the tree arrives whole on every
 * change, so a node is compared by what the item shows, not by identity. A
 * folder's contents are read when it opens, so they are not compared.
 */
function drawsTheSame(before: BarItemProps, after: BarItemProps): boolean {
  return (
    before.node.id === after.node.id &&
    before.title === after.title &&
    urlOf(before.node) === urlOf(after.node) &&
    isFolder(before.node) === isFolder(after.node) &&
    before.isPrivate === after.isPrivate &&
    before.isHidden === after.isHidden &&
    before.isExpanded === after.isExpanded &&
    before.onFollow === after.onFollow &&
    before.onOpenFolder === after.onOpenFolder &&
    before.onMenu === after.onMenu
  );
}

/** Drawn again only when what it shows changed, so editing one bookmark does not redraw the bar. */
const BarItem = memo(function BarItem({
  node,
  title,
  isPrivate,
  isHidden,
  isExpanded,
  onFollow,
  onOpenFolder,
  onMenu,
}: BarItemProps) {
  const url = urlOf(node);
  const onContextMenu = (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    onMenu(node, { x: event.clientX, y: event.clientY });
  };
  // A middle press would otherwise start the page's autoscroll.
  const onPointerDown = (event: PointerEvent) => {
    if (event.button === 1) {
      event.preventDefault();
    }
  };
  return (
    <li className={cx("flex shrink-0", isHidden && "invisible")} inert={isHidden}>
      <button
        type="button"
        className="btn ghost h-6 max-w-44 gap-1.5 px-1.5 text-xs font-normal"
        title={url === null ? title : `${title}\n${url}`}
        aria-haspopup={url === null ? "menu" : undefined}
        aria-expanded={url === null ? isExpanded : undefined}
        onPointerDown={onPointerDown}
        onClick={(event) => {
          if (url === null) {
            onOpenFolder(node, event.currentTarget);
          } else {
            onFollow(url, followOf(event));
          }
        }}
        onAuxClick={(event) => {
          if (url !== null && event.button === 1) {
            onFollow(url, "background");
          }
        }}
        onContextMenu={onContextMenu}
      >
        {url === null ? (
          <i className="ic-folder ic-sm text-text-secondary shrink-0" aria-hidden="true" />
        ) : (
          <Favicon key={url} url={isPrivate ? null : url} small />
        )}
        <span className="truncate">{title}</span>
      </button>
    </li>
  );
}, drawsTheSame);

interface BarFormProps {
  tree: Bookmarks;
  target: FormTarget;
  /** Where the menu that asked for it was opened. */
  at: Point;
  hold: BarContent;
  onDone: () => void;
}

/** The width of the bar's form, in pixels, so it can be kept inside the window. */
const FORM_WIDTH = 384;

/**
 * The form the bar's menus open, for **Add page…**, **Add folder…** and
 * **Edit…**: the bookmarks page's form, in a popover where the menu was.
 */
function BarForm({ tree, target, at, hold, onDone }: BarFormProps) {
  const formRef = useRef<HTMLElement>(null);
  useOverlay(formRef, true);
  useBarHold(hold, true);
  useMenuDismiss(true, onDone, formRef, null);
  const left = Math.max(4, Math.min(at.x, document.documentElement.clientWidth - FORM_WIDTH - 4));
  return createPortal(
    // On the overlay surface, which lets a little of the page through, and
    // without a shadow, which the input mask would cut off at its edge.
    <section
      ref={formRef}
      aria-label={t("bookmarks.menu")}
      tabIndex={-1}
      className="bg-chrome-overlay border-text/15 text-text fixed z-10 rounded-(--radius-surface) border outline-none"
      style={{ left, top: at.y, width: FORM_WIDTH }}
    >
      <BookmarkForm tree={tree} target={target} onDone={onDone} />
    </section>,
    document.body,
  );
}
