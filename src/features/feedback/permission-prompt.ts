/**
 * How a page's permission request is presented and how its answer is reported
 * back. [Permissions](../../../docs/specs/permissions.md).
 *
 * Kept free of React and of the toaster so the rules can be tested directly.
 */

import type { PermissionKind, PermissionRequest } from "@bindings";
import { placeAxis, type Box, type Point, type Size } from "@features/overlays/menu-model";
import { t, type TranslationKey } from "@shared/i18n";

/**
 * What a request asks to do, as the end of "… wants to".
 *
 * @param kind - What the page asked to use.
 * @returns The wording for it.
 */
export function permissionAction(kind: PermissionKind): string {
  return t(`permission.ask.${kind}` satisfies TranslationKey);
}

/**
 * A permission's short name, as settings lists it.
 *
 * @param kind - The permission.
 * @returns Its name.
 */
export function permissionName(kind: PermissionKind): string {
  return t(`permission.name.${kind}` satisfies TranslationKey);
}

/**
 * The title a request is shown under. It names the origin that asked, so a
 * page cannot dress a request up as one of Haku's own or as another site's.
 *
 * @param request - The request shown.
 * @returns The dialog's title.
 */
export function permissionTitle(request: PermissionRequest): string {
  // Replaced through functions, and the origin last, so neither value is read
  // as a replacement pattern or a placeholder.
  return t("permission.title")
    .replace("{action}", () => permissionAction(request.kind))
    .replace("{origin}", () => request.origin);
}

/** How far the prompt stands off what it is anchored to. */
const GAP = 6;

/** The prompt's border, padding and text inset, so its text starts under the glyph. */
export const PROMPT_INSET = 13;

/** What the prompt is anchored to, measured from the site menu's button. */
export interface PromptAnchor {
  /** The address bar holding the site menu's button. */
  field: Box;
  /** The site menu's glyph, which the prompt's text starts under. */
  glyph: Box;
  /** The vertical sidebar holding the address bar, when the toolbar is in one. */
  sidebar: Box | null;
}

/**
 * Where the permission prompt goes.
 *
 * Under the site menu's glyph when the toolbar runs across the top, as the
 * site menu opens. In a vertical sidebar, beside the sidebar at the address
 * bar's height, over the page: under the glyph it would cover the tabs, and
 * run off the window from a sidebar on the right. With no site menu on
 * screen, as in fullscreen, at the top centre of the window.
 * [Permissions § Behaviour](../../../docs/specs/permissions.md#behaviour).
 *
 * @param anchor - The site menu's button, or null when none is on screen.
 * @param size - The prompt's size.
 * @param bounds - The window's size.
 * @returns The prompt's top-left corner.
 */
export function placePrompt(anchor: PromptAnchor | null, size: Size, bounds: Size): Point {
  if (anchor === null) {
    return {
      x: placeAxis((bounds.width - size.width) / 2, 0, size.width, bounds.width),
      y: placeAxis(GAP, 0, size.height, bounds.height),
    };
  }
  const { field, glyph, sidebar } = anchor;
  if (sidebar) {
    return {
      x: placeAxis(sidebar.right + GAP, sidebar.left - GAP - size.width, size.width, bounds.width),
      y: placeAxis(field.top, field.bottom - size.height, size.height, bounds.height),
    };
  }
  return {
    x: placeAxis(glyph.left - PROMPT_INSET, glyph.right + PROMPT_INSET - size.width, size.width, bounds.width),
    y: placeAxis(field.bottom + GAP, field.top - GAP - size.height, size.height, bounds.height),
  };
}
