import { useEffect, type RefObject } from "react";

/** Space between a menu's button and the menu below it, in pixels. */
export const MENU_GAP = 6;

/**
 * Closes a menu the way a menu should close, and makes it reachable by keyboard.
 *
 * On open, focus moves to the menu itself, since the menu is portalled out of
 * its button and would otherwise be unreachable by keyboard; Tab then reaches
 * its controls. The menu, not its first control, takes focus so that opening
 * it does not also open a tooltip or toggle that happens to come first. The
 * menu element needs `tabIndex={-1}` to take focus.
 *
 * Escape closes it and returns focus to the button. A click outside, a window
 * blur or a resize closes it without moving focus, because the person has
 * already moved it elsewhere.
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
    menuRef.current?.focus();
  }, [open, menuRef]);

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
        buttonRef.current?.focus();
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
