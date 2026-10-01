import { useEffect, useId, type RefObject } from "react";

import { rectOf, registerOverlay, unregisterOverlay } from "./registry";

/**
 * Keeps a floating element clickable while it is open.
 *
 * Registers the element's rectangle with the overlay store for as long as
 * `open` is true, and follows it as it resizes or the window changes. Any
 * component that renders over page content must use this; without it the
 * element paints correctly but every click lands on the page beneath.
 *
 * @param ref - The floating element.
 * @param open - Whether it is currently shown.
 */
export function useOverlay(ref: RefObject<HTMLElement | null>, open: boolean): void {
  const id = useId();

  useEffect(() => {
    const element = ref.current;
    if (!open || !element) {
      unregisterOverlay(id);
      return;
    }

    const measure = () => registerOverlay(id, rectOf(element));
    measure();

    const observer = new ResizeObserver(measure);
    observer.observe(element);
    window.addEventListener("resize", measure);

    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
      unregisterOverlay(id);
    };
  }, [id, open, ref]);
}
