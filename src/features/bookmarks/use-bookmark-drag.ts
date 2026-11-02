import type { BookmarkId, Bookmarks } from "@bindings";
import { commands } from "@ipc/commands";
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";

import { childrenOf, contains, isRoot, locate, moveIndex } from "./tree";

/** How far the pointer moves, in CSS pixels, before a press becomes a drag. */
const DRAG_THRESHOLD = 5;

/**
 * Where a dragged bookmark would land: before or after a row, or into a
 * folder, at its end.
 */
export type DropTarget =
  | {
      kind: "before" | "after";
      row: BookmarkId;
      parent: BookmarkId;
      slot: number;
    }
  | { kind: "into"; folder: BookmarkId };

interface Press {
  id: BookmarkId;
  pointerId: number;
  x: number;
  y: number;
  started: boolean;
}

/**
 * Elements a bookmark can be dropped on carry, as data attributes:
 * `data-drop-row` (the bookmark's id), `data-drop-parent` and `data-drop-slot`
 * (its folder and place there) on a row, with `data-drop-folder` too when the
 * row is a folder; and `data-drop-folder` alone on a folder in the tree.
 */
function targetAt(x: number, y: number): DropTarget | null {
  const element = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop-row], [data-drop-folder]");
  if (!element) {
    return null;
  }
  const { dropRow, dropParent, dropSlot, dropFolder } = element.dataset;
  if (dropRow === undefined || dropParent === undefined || dropSlot === undefined) {
    return dropFolder === undefined ? null : { kind: "into", folder: Number(dropFolder) };
  }
  const box = element.getBoundingClientRect();
  const fraction = box.height > 0 ? (y - box.top) / box.height : 0;
  // A folder row takes its middle half as a drop into it.
  if (dropFolder !== undefined && fraction > 0.25 && fraction < 0.75) {
    return { kind: "into", folder: Number(dropFolder) };
  }
  const slot = Number(dropSlot);
  const after = fraction >= 0.5;
  return {
    kind: after ? "after" : "before",
    row: Number(dropRow),
    parent: Number(dropParent),
    slot: after ? slot + 1 : slot,
  };
}

/** Whether `moving` may go to `target`: never into itself or what it holds, and never beside a root. */
function allowed(tree: Bookmarks, moving: BookmarkId, target: DropTarget): boolean {
  const node = locate(tree, moving)?.node;
  if (!node || isRoot(tree, moving)) {
    return false;
  }
  const parent = target.kind === "into" ? target.folder : target.parent;
  if (target.kind !== "into" && target.row === moving) {
    return false;
  }
  return !contains(node, parent);
}

/**
 * Dragging bookmarks with the pointer, to reorder them or move them into a
 * folder. Pointer-driven rather than HTML drag and drop, as the tab strip's
 * is: enabling HTML drops in the chrome lets an external drop navigate it
 * ([`tauri/AGENTS.md`](../../../tauri/AGENTS.md)).
 *
 * @param tree - The tree as it is now, or null before it is known.
 */
export function useBookmarkDrag(tree: Bookmarks | null) {
  const [dragging, setDragging] = useState<BookmarkId | null>(null);
  const [target, setTarget] = useState<DropTarget | null>(null);
  const press = useRef<Press | null>(null);
  const suppressClick = useRef(false);
  const treeRef = useRef(tree);
  useEffect(() => {
    treeRef.current = tree;
  });

  const clear = () => {
    press.current = null;
    setDragging(null);
    setTarget(null);
  };

  useEffect(() => {
    const resolve = (event: PointerEvent): DropTarget | null => {
      const current = press.current;
      const latest = treeRef.current;
      if (!current || !latest) {
        return null;
      }
      const found = targetAt(event.clientX, event.clientY);
      return found && allowed(latest, current.id, found) ? found : null;
    };
    const move = (event: PointerEvent) => {
      const current = press.current;
      if (!current || current.pointerId !== event.pointerId) {
        return;
      }
      if (!current.started && Math.hypot(event.clientX - current.x, event.clientY - current.y) < DRAG_THRESHOLD) {
        return;
      }
      if (!current.started) {
        current.started = true;
        setDragging(current.id);
      }
      event.preventDefault();
      setTarget(resolve(event));
    };
    const up = (event: PointerEvent) => {
      const current = press.current;
      if (!current || current.pointerId !== event.pointerId) {
        return;
      }
      const drop = current.started ? resolve(event) : null;
      const latest = treeRef.current;
      suppressClick.current = current.started;
      clear();
      if (!drop || !latest) {
        return;
      }
      if (drop.kind === "into") {
        void commands.moveBookmark(current.id, drop.folder, null);
        return;
      }
      const siblings = childrenOf(locate(latest, drop.parent)?.node ?? latest.bar);
      void commands.moveBookmark(current.id, drop.parent, moveIndex(siblings, current.id, drop.slot));
    };
    const cancel = (event: PointerEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent ? event.key === "Escape" : press.current?.pointerId === event.pointerId) {
        clear();
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", cancel);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", cancel);
    };
  }, []);

  return {
    dragging,
    target,
    /** Starts following a press on a bookmark's row. */
    start(id: BookmarkId, event: ReactPointerEvent) {
      if (event.button !== 0 || !event.isPrimary || press.current) {
        return;
      }
      suppressClick.current = false;
      press.current = { id, pointerId: event.pointerId, x: event.clientX, y: event.clientY, started: false };
    },
    /** Whether the click that ends a drag should be ignored; asking clears it. */
    consumeClick(): boolean {
      const suppress = suppressClick.current;
      suppressClick.current = false;
      return suppress;
    },
  };
}

/** Whether a folder is where a drop would land, for highlighting it. */
export function isDropInto(target: DropTarget | null, folder: BookmarkId): boolean {
  return target?.kind === "into" && target.folder === folder;
}

/** Which edge of a row a drop would land at, if any. */
export function dropEdge(target: DropTarget | null, row: BookmarkId): "before" | "after" | null {
  return target && target.kind !== "into" && target.row === row ? target.kind : null;
}
