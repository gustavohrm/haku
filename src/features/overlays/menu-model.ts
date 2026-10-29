/**
 * What a menu holds and how it moves, apart from how it is drawn.
 *
 * Kept free of the DOM so placement and keyboard movement are tested directly.
 */

interface ItemBase {
  /** Unique within its menu level. */
  id: string;
  label: string;
  /** An icon class, such as `ic-copy`, written out in full where it is set. */
  icon?: string;
  /** The key that does the same, shown as a hint. */
  shortcut?: string | null;
  disabled?: boolean;
}

export type MenuItem =
  | (ItemBase & { kind: "command"; onSelect: () => void })
  | (ItemBase & { kind: "checkbox" | "radio"; checked: boolean; onSelect: () => void })
  | (ItemBase & { kind: "submenu"; items: readonly MenuItem[] })
  | { kind: "separator"; id: string };

export type ChoosableItem = Exclude<MenuItem, { kind: "separator" }>;

/** Keeps a menu this far from the window's edges, in pixels. */
export const MENU_EDGE = 4;

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/**
 * Places a span along one axis: at `start` when it fits before `limit`, at
 * `flipped` when it does not, and then clamped inside the window.
 */
function placeAxis(start: number, flipped: number, size: number, limit: number): number {
  const position = start + size > limit - MENU_EDGE ? flipped : start;
  const furthest = Math.max(MENU_EDGE, limit - MENU_EDGE - size);
  return Math.min(Math.max(position, MENU_EDGE), furthest);
}

/**
 * Where a menu opened at the pointer goes: down and to the right of it,
 * flipped left or up where it would leave the window, then clamped to it.
 */
export function placeAtPoint(at: Point, size: Size, bounds: Size): Point {
  return {
    x: placeAxis(at.x, at.x - size.width, size.width, bounds.width),
    y: placeAxis(at.y, at.y - size.height, size.height, bounds.height),
  };
}

/**
 * Where a submenu goes: beside its item, level with its top, flipped to the
 * other side or upwards where it would leave the window, then clamped to it.
 */
export function placeBeside(item: Box, size: Size, bounds: Size): Point {
  return {
    x: placeAxis(item.right, item.left - size.width, size.width, bounds.width),
    y: placeAxis(item.top, item.bottom - size.height, size.height, bounds.height),
  };
}

/** Whether an item can be moved to and chosen. */
export function isChoosable(item: MenuItem | undefined): item is ChoosableItem {
  return item !== undefined && item.kind !== "separator" && !item.disabled;
}

/**
 * The next choosable item from `from` in direction `step`, wrapping around.
 * `from` may be -1, for a menu with nothing highlighted yet.
 *
 * @returns Its index, or -1 when nothing in the menu can be chosen.
 */
export function stepIndex(items: readonly MenuItem[], from: number, step: 1 | -1): number {
  const count = items.length;
  for (let offset = 1; offset <= count; offset++) {
    const index = (((from + step * offset) % count) + count) % count;
    if (isChoosable(items[index])) {
      return index;
    }
  }
  return -1;
}

/** The first choosable item, or -1. */
export function firstIndex(items: readonly MenuItem[]): number {
  return stepIndex(items, -1, 1);
}

/** The last choosable item, or -1. */
export function lastIndex(items: readonly MenuItem[]): number {
  return stepIndex(items, 0, -1);
}

/**
 * The next choosable item after `from` whose label starts with `letter`,
 * wrapping around, as typing a letter in a menu jumps.
 *
 * @returns Its index, or `from` when no item starts with it.
 */
export function indexByLetter(items: readonly MenuItem[], from: number, letter: string): number {
  const wanted = letter.toLocaleLowerCase();
  const count = items.length;
  for (let offset = 1; offset <= count; offset++) {
    const index = (((from + offset) % count) + count) % count;
    const item = items[index];
    if (isChoosable(item) && item.label.trimStart().toLocaleLowerCase().startsWith(wanted)) {
      return index;
    }
  }
  return from;
}

/**
 * Drops separators that would separate nothing: at either end, or next to
 * another separator, as happens when the items around one are left out.
 */
export function tidySeparators(items: readonly MenuItem[]): MenuItem[] {
  const tidy: MenuItem[] = [];
  for (const item of items) {
    const previous = tidy.at(-1);
    if (item.kind === "separator" && (previous === undefined || previous.kind === "separator")) {
      continue;
    }
    tidy.push(item);
  }
  if (tidy.at(-1)?.kind === "separator") {
    tidy.pop();
  }
  return tidy;
}
