/**
 * Notifications and dialogs, through one `@codenhub/toaster` instance.
 *
 * Everything the interface tells the user outside of its normal layout goes
 * through here: failures, and the dialogs pages open. Nothing uses the
 * browser's own `alert`, `confirm` or `prompt`.
 *
 * Toasts and dialogs float over page content, which is a hole in the chrome's
 * input mask, so they are only visible and clickable once registered as
 * overlays. The instance renders into a container this module owns, and a
 * single observer keeps every toast stack and open dialog registered.
 */

import type { HakuError } from "@bindings";
import { createToaster, type Toaster } from "@codenhub/toaster";
import { rectOf, registerOverlay, unregisterOverlay } from "@features/overlays/registry";
import { describeError } from "@ipc/result";
import { t } from "@shared/i18n";

/** A failure repeated within this window is shown once. */
const REPEAT_WINDOW_MS = 5_000;

let instance: Toaster | null = null;
const lastShown = new Map<string, number>();

/**
 * The toaster, created on first use.
 *
 * @returns The one instance the interface uses.
 */
export function feedback(): Toaster {
  if (instance) {
    return instance;
  }

  const container = document.createElement("div");
  container.dataset.hakuFeedback = "";
  document.body.append(container);

  instance = createToaster({
    container,
    position: "bottom-right",
    labels: {
      dismiss: t("feedback.dismiss"),
      confirm: t("dialog.ok"),
      cancel: t("dialog.cancel"),
      submit: t("dialog.ok"),
      ok: t("dialog.ok"),
    },
  });
  trackAsOverlays(container);
  return instance;
}

/**
 * Tells the user a command failed.
 *
 * Some commands run on a timer, so a persistent failure would otherwise repeat
 * the same toast indefinitely.
 *
 * @param error - The failure Rust reported.
 */
export function notifyFailure(error: HakuError): void {
  const description = describeError(error);
  const now = Date.now();
  const previous = lastShown.get(description);
  if (previous !== undefined && now - previous < REPEAT_WINDOW_MS) {
    return;
  }
  lastShown.set(description, now);
  feedback().error({ title: t("error.title"), description });
}

/**
 * Keeps whatever the toaster shows registered as an overlay.
 *
 * Each direct child of the container is a toast stack or a dialog. Stacks are
 * registered while they hold a toast and dialogs while they are open; both are
 * re-measured as they change size or finish animating.
 */
function trackAsOverlays(container: HTMLElement): void {
  const ids = new WeakMap<Element, string>();
  let registered = new Set<string>();
  let counter = 0;
  let frame = 0;

  const resize = new ResizeObserver(() => schedule());

  const measure = () => {
    const shown = new Set<string>();
    for (const element of container.children) {
      let id = ids.get(element);
      if (!id) {
        id = `feedback-${counter++}`;
        ids.set(element, id);
        resize.observe(element);
      }
      const visible = element instanceof HTMLDialogElement ? element.open : element.childElementCount > 0;
      if (visible) {
        registerOverlay(id, rectOf(element));
        shown.add(id);
      }
    }
    for (const id of registered) {
      if (!shown.has(id)) {
        unregisterOverlay(id);
      }
    }
    registered = shown;
  };

  const schedule = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(measure);
  };

  new MutationObserver(schedule).observe(container, { childList: true, subtree: true, attributes: true });
  container.addEventListener("transitionend", schedule, true);
  container.addEventListener("animationend", schedule, true);
  window.addEventListener("resize", schedule);
}
