/**
 * Tracks what the interface is currently drawing over page content.
 *
 * The chrome webview sits above every content webview and would otherwise
 * swallow all input, so Rust masks it down to the regions the interface
 * actually occupies. Anything that floats over the viewport — a menu, a
 * dropdown, a dialog — has to register itself here or it will be visible but
 * not clickable.
 */

import type { Overlay, Viewport } from "@bindings";
import { createStore } from "@ipc/store";

const overlays = new Map<string, Overlay>();

export const overlayStore = createStore<readonly Overlay[]>([]);

function publish(): void {
  overlayStore.set([...overlays.values()]);
}

/**
 * Records the region an overlay occupies, replacing any previous one.
 *
 * @param id - Stable identity for this overlay.
 * @param overlay - Where it sits, in logical pixels relative to the window,
 *   and how round its corners are.
 */
export function registerOverlay(id: string, overlay: Overlay): void {
  const existing = overlays.get(id);
  if (existing && sameOverlay(existing, overlay)) {
    return;
  }
  overlays.set(id, overlay);
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

function sameOverlay(a: Overlay, b: Overlay): boolean {
  return (
    a.radius === b.radius &&
    a.rect.x === b.rect.x &&
    a.rect.y === b.rect.y &&
    a.rect.width === b.rect.width &&
    a.rect.height === b.rect.height
  );
}

/** Reads an element's position in the logical pixels Rust expects. */
export function rectOf(element: Element): Viewport {
  const { x, y, width, height } = element.getBoundingClientRect();
  return { x, y, width, height };
}

/**
 * Reads the region an element covers over the page.
 *
 * The corner radius comes from the element's own style, so a rounded overlay
 * gets a rounded region and its corners do not paint chrome over the page.
 * Only the top-left corner is read: overlays round all four alike.
 */
export function overlayOf(element: Element): Overlay {
  const radius = Number.parseFloat(getComputedStyle(element).borderTopLeftRadius) || 0;
  return { rect: rectOf(element), radius };
}
