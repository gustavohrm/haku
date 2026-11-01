/**
 * How a page's permission request is presented and how its answer is reported
 * back. [Permissions](../../../docs/specs/permissions.md).
 *
 * Kept free of React and of the toaster so the rules can be tested directly.
 */

import type { PermissionDecision, PermissionKind, PermissionRequest } from "@bindings";
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
  return t("permission.title").replace("{origin}", request.origin).replace("{action}", permissionAction(request.kind));
}

/**
 * Turns what the toaster resolved to into the decision sent back.
 *
 * The toaster resolves Block and Escape to the same `false`, so whether the
 * dialog was cancelled with Escape is told apart by the caller.
 *
 * @param result - What the confirmation resolved to.
 * @param escaped - Whether the dialog was closed with Escape.
 * @returns The decision to send.
 */
export function toDecision(result: boolean, escaped: boolean): PermissionDecision {
  if (result) {
    return "allow";
  }
  return escaped ? "dismiss" : "block";
}
