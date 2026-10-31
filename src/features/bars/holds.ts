import { createStore } from "@ipc/store";
import { useEffect } from "react";

/** What a floating bar holds: a group shows while any of its contents is held. */
export type BarContent = "tabs" | "toolbar";

type Holds = Readonly<Record<BarContent, number>>;

/** How many things keep each kind of bar content on screen. */
export const barHoldStore = createStore<Holds>({ tabs: 0, toolbar: 0 });

function adjust(content: BarContent, by: number) {
  const current = barHoldStore.get();
  barHoldStore.set({ ...current, [content]: current[content] + by });
}

/**
 * Keeps the floating bar holding `content` shown, as a menu opened from it
 * must, until the returned release is called. Releasing twice is harmless.
 */
export function holdBar(content: BarContent): () => void {
  adjust(content, 1);
  let isHeld = true;
  return () => {
    if (isHeld) {
      isHeld = false;
      adjust(content, -1);
    }
  };
}

/** Holds the bar while `active`, for a menu that lives in a component. */
export function useBarHold(content: BarContent, active: boolean): void {
  useEffect(() => (active ? holdBar(content) : undefined), [content, active]);
}

/**
 * Holds the bar for a window of its own opened from it, such as an
 * extension's toolbar popup, which Rust does not report closing.
 *
 * The popup takes focus from the chrome. Once it has, the chrome seeing the
 * pointer or focus again means the popup is gone or no longer being used. A
 * click in the chrome ends it at any time, as clicking the popup's button
 * again closes the popup.
 *
 * @returns Releases the hold early, as when the popup failed to open.
 */
export function holdBarForWindow(content: BarContent): () => void {
  const release = holdBar(content);
  let hasLeft = false;
  const onBlur = () => {
    hasLeft = true;
  };
  const onReturn = () => {
    if (hasLeft) {
      stop();
    }
  };
  const stop = () => {
    window.removeEventListener("blur", onBlur);
    window.removeEventListener("focus", onReturn);
    document.removeEventListener("pointermove", onReturn);
    document.removeEventListener("pointerdown", stop, true);
    release();
  };
  window.addEventListener("blur", onBlur);
  window.addEventListener("focus", onReturn);
  document.addEventListener("pointermove", onReturn);
  document.addEventListener("pointerdown", stop, true);
  return stop;
}
