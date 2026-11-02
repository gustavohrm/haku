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

import type { HakuError, Overlay } from "@bindings";
import { createToaster, type Toaster } from "@codenhub/toaster";
import { overlayOf, registerOverlay, unregisterOverlay } from "@features/overlays/registry";
import { describeError, HakuFailure } from "@ipc/result";
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
    // A failure can take longer to read than it stays up, so it can be closed
    // by hand too, with the mouse or the keyboard.
    dismissible: true,
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
 * A persistent failure tends to repeat as the user retries, and would otherwise
 * stack the same toast again and again.
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
 * Tells the user about a failure nothing has reported yet.
 *
 * A failed command was already reported by the command wrapper, so only
 * anything else, such as an exception from the IPC layer, is shown here.
 *
 * @param error - What was thrown.
 */
export function reportUnexpected(error: unknown): void {
  if (!(error instanceof HakuFailure)) {
    feedback().error({ title: t("error.title"), description: String(error) });
  }
}

/**
 * Keeps whatever the toaster shows registered as an overlay.
 *
 * Each direct child of the container is a toast stack or a dialog. A dialog is
 * registered while it is open and re-measured as it changes size or finishes
 * animating.
 *
 * A stack is not registered itself: it is square and spans the gaps between
 * its toasts, so the chrome would show at the toasts' rounded corners and
 * between them. Each toast is registered instead, where it is painted. Toasts
 * slide in, out and along the stack, and the toaster starts an exit without
 * changing the DOM, so while any toast is shown it is measured every frame. An
 * unchanged region is not reported again, so a toast at rest costs a
 * measurement and nothing more.
 */
function trackAsOverlays(container: HTMLElement): void {
  const ids = new WeakMap<Element, string>();
  const observed = new WeakSet<Element>();
  let registered = new Set<string>();
  let counter = 0;
  let frame = 0;

  const resize = new ResizeObserver(() => schedule());

  const measure = () => {
    frame = 0;
    const shown = new Set<string>();
    let hasToasts = false;
    const show = (element: Element, overlay: Overlay) => {
      let id = ids.get(element);
      if (!id) {
        id = `feedback-${counter++}`;
        ids.set(element, id);
      }
      registerOverlay(id, overlay);
      shown.add(id);
    };
    for (const element of container.children) {
      if (!observed.has(element)) {
        observed.add(element);
        resize.observe(element);
      }
      if (element instanceof HTMLDialogElement) {
        if (element.open) {
          show(element, overlayOf(element));
        }
        continue;
      }
      for (const toast of element.children) {
        hasToasts = true;
        show(toast, toastOverlay(element, toast));
      }
    }
    for (const id of registered) {
      if (!shown.has(id)) {
        unregisterOverlay(id);
      }
    }
    registered = shown;
    if (hasToasts) {
      schedule();
    }
  };

  const schedule = () => {
    if (!frame) {
      frame = requestAnimationFrame(measure);
    }
  };

  new MutationObserver(schedule).observe(container, { childList: true, subtree: true, attributes: true });
  container.addEventListener("transitionend", schedule, true);
  container.addEventListener("animationend", schedule, true);
  window.addEventListener("resize", schedule);
}

/**
 * Reads where a toast is painted, with its own corners.
 *
 * Clipped to its stack, which hides a toast sliding in or out past its edge
 * and scrolls what does not fit.
 */
function toastOverlay(stack: Element, toast: Element): Overlay {
  const clip = stack.getBoundingClientRect();
  const box = toast.getBoundingClientRect();
  const left = Math.max(box.left, clip.left);
  const top = Math.max(box.top, clip.top);
  const right = Math.min(box.right, clip.right);
  const bottom = Math.min(box.bottom, clip.bottom);
  return {
    rect: { x: left, y: top, width: Math.max(0, right - left), height: Math.max(0, bottom - top) },
    radius: overlayOf(toast).radius,
  };
}
