import { cx } from "@shared/class-names";

/**
 * Classes of a window control, shared with the startup failure screen, which
 * draws the controls without React.
 *
 * @param danger - Whether it closes the window, which hovers red.
 * @returns The button's class attribute.
 */
export function windowButtonClass(danger = false): string {
  return cx(
    "grid w-11 place-items-center text-text-secondary transition-colors hover:text-text",
    danger ? "hover:bg-destructive hover:text-destructive-contrast" : "hover:bg-text/10",
  );
}
