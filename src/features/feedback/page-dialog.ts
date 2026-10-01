/**
 * How a page's dialog is presented and how its outcome is reported back.
 *
 * Kept free of React and of the toaster so the rules can be tested directly.
 */

import type { DialogAnswer, DialogKind } from "@bindings";
import { t } from "@shared/i18n";

/**
 * The title a page's dialog is shown under: the site that opened it.
 *
 * The chrome draws these dialogs itself, so the title is what stops a page from
 * dressing a message up as one of Haku's own.
 *
 * @param url - The page that opened the dialog.
 * @returns A title naming its host.
 */
export function dialogTitle(url: string): string {
  let host = "";
  try {
    host = new URL(url).host;
  } catch {
    // An unparseable URL still gets a title; it just cannot name a site.
  }
  return host ? t("dialog.from").replace("{host}", host) : t("dialog.fromPage");
}

/**
 * Turns what the toaster resolved to into the answer the page receives.
 *
 * The toaster resolves an alert to nothing, a confirmation to a boolean, and a
 * prompt to its text or `null` when cancelled.
 *
 * @param kind - The kind of dialog that was shown.
 * @param result - What its handle resolved to.
 * @returns The answer to send back to the page.
 */
export function toAnswer(kind: DialogKind, result: unknown): DialogAnswer {
  switch (kind) {
    case "alert":
      return { action: "accept", text: null };
    case "prompt":
      return typeof result === "string" ? { action: "accept", text: result } : { action: "dismiss" };
    case "confirm":
    case "beforeUnload":
      return result === true ? { action: "accept", text: null } : { action: "dismiss" };
  }
}
