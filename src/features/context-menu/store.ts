import type { MenuItem } from "@features/overlays/menu";
import { useSyncExternalStore } from "react";

export interface MenuRequest {
  items: readonly MenuItem[];
  /** Where the pointer was, in the window's CSS pixels. */
  at: { x: number; y: number };
  label: string;
  /**
   * Called once when the menu closes, after the chosen item's `onSelect`.
   * `fromInside` is true for a choice, Escape or Tab, and false otherwise,
   * including when another menu or the code that opened it closes it.
   */
  onClose?: (fromInside: boolean) => void;
}

export interface OpenMenu extends MenuRequest {
  key: number;
}

let current: OpenMenu | null = null;
let nextKey = 1;
const listeners = new Set<() => void>();

function publish(next: OpenMenu | null) {
  current = next;
  for (const listener of listeners) {
    listener();
  }
}

/**
 * Opens a context menu, closing the one already open: a window shows one at a
 * time.
 *
 * @returns Closes this menu, and does nothing once another has replaced it.
 */
export function openMenu(request: MenuRequest): () => void {
  closeMenu();
  const key = nextKey++;
  publish({ ...request, key });
  return () => closeMenu(key);
}

/**
 * Closes the open menu, or only the menu `key` when given.
 *
 * @param fromInside - The menu closed from inside, as `onClose` reports.
 */
export function closeMenu(key?: number, fromInside = false): void {
  const open = current;
  if (open === null || (key !== undefined && open.key !== key)) {
    return;
  }
  publish(null);
  open.onClose?.(fromInside);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The context menu open in this window, if any. */
export function useOpenMenu(): OpenMenu | null {
  return useSyncExternalStore(subscribe, () => current);
}
