import { useEffect, useId, type RefObject } from "react";

import { overlayOf, registerOverlay, unregisterOverlay } from "./registry";

/** The longest an overlay is followed frame by frame, in case an animation never reports its end. */
const MAX_FOLLOW_MS = 1000;

/** Whether the element is mid-transition or mid-animation. Engines without the API report none. */
function isAnimating(element: HTMLElement): boolean {
  return typeof element.getAnimations === "function" && element.getAnimations().length > 0;
}

/**
 * Measures `element` every frame while it animates, then calls `settled`.
 *
 * A transform is not a resize, so nothing else reports an element sliding into
 * place. Following it each frame keeps the region Rust masks in step with
 * what the chrome paints, rather than catching up once it stops.
 *
 * @returns Stops following.
 */
function follow(element: HTMLElement, measure: () => void, settled: () => void): () => void {
  const started = performance.now();
  let frame = 0;
  const step = () => {
    if (isAnimating(element) && performance.now() - started < MAX_FOLLOW_MS) {
      measure();
      frame = requestAnimationFrame(step);
    } else {
      settled();
    }
  };
  step();
  return () => cancelAnimationFrame(frame);
}

/**
 * Keeps a floating element clickable while it is open.
 *
 * Registers the element's region with the overlay store for as long as
 * `open` is true, and follows it as it resizes, moves or the window changes.
 * An element that animates into place, such as a bar sliding in, is followed
 * frame by frame until it settles. One that animates away once `open` turns
 * false stays registered, followed the same way, until it is gone, so it is not
 * cut off under the page mid-exit. Any component that renders over page
 * content must use this; without it the element paints correctly but every
 * click lands on the page beneath.
 *
 * @param ref - The floating element.
 * @param open - Whether it is currently shown.
 */
export function useOverlay(ref: RefObject<HTMLElement | null>, open: boolean): void {
  const id = useId();

  useEffect(() => {
    const element = ref.current;
    if (!element) {
      unregisterOverlay(id);
      return;
    }
    const measure = () => registerOverlay(id, overlayOf(element));

    if (!open) {
      // Leaving: followed while it animates away, then released.
      const stop = follow(element, measure, () => unregisterOverlay(id));
      return () => {
        stop();
        unregisterOverlay(id);
      };
    }

    measure();
    let stop = follow(element, measure, measure);
    const restart = () => {
      stop();
      stop = follow(element, measure, measure);
    };

    const observer = new ResizeObserver(measure);
    observer.observe(element);
    // An element placed within its parent, such as the find bar in a split
    // pane, moves without resizing when the parent does.
    if (element.parentElement) {
      observer.observe(element.parentElement);
    }
    window.addEventListener("resize", measure);
    element.addEventListener("transitionrun", restart);
    element.addEventListener("animationstart", restart);
    element.addEventListener("transitionend", measure);
    element.addEventListener("animationend", measure);

    return () => {
      stop();
      observer.disconnect();
      window.removeEventListener("resize", measure);
      element.removeEventListener("transitionrun", restart);
      element.removeEventListener("animationstart", restart);
      element.removeEventListener("transitionend", measure);
      element.removeEventListener("animationend", measure);
      unregisterOverlay(id);
    };
  }, [id, open, ref]);
}
