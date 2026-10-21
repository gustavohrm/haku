import { useEffect, type RefObject } from "react";

/** Space between a menu's button and the menu below it, in pixels. */
export const MENU_GAP = 6;

/**
 * Closes a menu on Escape, on a pointer down outside it and its button, and
 * when the window loses focus or is resized.
 *
 * @param open - Whether the menu is shown.
 * @param close - Hides the menu. Should be stable across renders.
 * @param menuRef - The menu element.
 * @param buttonRef - The button that opens the menu.
 */
export function useMenuDismiss(
  open: boolean,
  close: () => void,
  menuRef: RefObject<HTMLElement | null>,
  buttonRef: RefObject<HTMLElement | null>,
): void {
  useEffect(() => {
    if (!open) {
      return;
    }
    const closeOutside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!menuRef.current?.contains(target) && !buttonRef.current?.contains(target)) {
        close();
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        close();
      }
    };
    // A click on the page never reaches the chrome; it only takes focus away.
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open, close, menuRef, buttonRef]);
}
