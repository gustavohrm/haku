/**
 * Tracks what the interface is currently drawing over page content.
 *
 * The chrome webview sits above every content webview and would otherwise
 * swallow all input, so Rust masks it down to the rectangles the interface
 * actually occupies. Anything that floats over the viewport — a menu, a
 * dropdown, a dialog — has to register itself here or it will be visible but
 * not clickable.
 */

import type { Viewport } from "@bindings";
import { createStore } from "@ipc/store";

const overlays = new Map<string, Viewport>();

export const overlayStore = createStore<readonly Viewport[]>([]);

function publish(): void {
  overlayStore.set([...overlays.values()]);
}

/**
 * Records the rectangle an overlay occupies, replacing any previous one.
 *
 * @param id - Stable identity for this overlay.
 * @param rect - Where it sits, in logical pixels relative to the window.
 */
export function registerOverlay(id: string, rect: Viewport): void {
  const existing = overlays.get(id);
  if (existing && sameRect(existing, rect)) {
    return;
  }
  overlays.set(id, rect);
  publish();
}

/**
 * Removes an overlay, restoring the page's claim on that area.
 *
 * @param id - The identity used when registering.
 */
export function unregisterOverlay(id: string): void {
  if (overlays.delete(id)) {
    publish();
  }
}

function sameRect(a: Viewport, b: Viewport): boolean {
  return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height;
}

/** Reads an element's position in the logical pixels Rust expects. */
export function rectOf(element: Element): Viewport {
  const { x, y, width, height } = element.getBoundingClientRect();
  return { x, y, width, height };
}
