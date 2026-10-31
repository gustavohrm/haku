import { useEffect, useRef, useState, useSyncExternalStore, type FocusEvent, type PointerEvent } from "react";

import { barHoldStore, type BarContent } from "./holds";

/**
 * How long the pointer must rest at a hidden bar's edge before the bar shows,
 * so a pointer passing over the edge on its way somewhere else does not.
 */
export const SHOW_DELAY_MS = 150;
/** How long a floating bar stays after the pointer leaves it, so crossing back does not flicker it. */
export const HIDE_DELAY_MS = 300;

interface Reveal {
  revealed: boolean;
  /** Spread on the element holding the bar and its edge. */
  handlers: {
    onPointerEnter: () => void;
    onPointerLeave: (event: PointerEvent<HTMLElement>) => void;
    onFocus: () => void;
    onBlur: (event: FocusEvent<HTMLElement>) => void;
  };
}

/**
 * Whether a floating bar is shown: once the pointer has rested on its edge,
 * while the pointer is on the shown bar, while anything in it has focus, while
 * something opened from it holds it, or while something in it needs attention.
 * [Appearance § Bars](../../../docs/specs/appearance.md#bars).
 *
 * @param contents - What the bar holds, so holds on it count.
 * @param isNeeded - Something in it needs attention without a pointer.
 * @param floating - Whether the bar floats; a docked bar is already on screen.
 */
export function useReveal(contents: readonly BarContent[], isNeeded: boolean, floating: boolean): Reveal {
  const [isHovered, setHovered] = useState(false);
  const [isFocused, setFocused] = useState(false);
  const holds = useSyncExternalStore(barHoldStore.subscribe, barHoldStore.get);
  const showTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(
    () => () => {
      clearTimeout(showTimer.current);
      clearTimeout(hideTimer.current);
    },
    [],
  );

  const isHeld = contents.some((content) => holds[content] > 0);
  const revealed = isHovered || isFocused || isHeld || isNeeded;
  return {
    revealed,
    handlers: {
      onPointerEnter: () => {
        clearTimeout(hideTimer.current);
        clearTimeout(showTimer.current);
        // A bar already on screen, shown or docked, keeps the pointer at
        // once; only reaching for a hidden one waits, since then the pointer
        // can only be at the edge. A docked bar is tracked without delay, so
        // no timer started there is left to show the bar once it floats.
        if (revealed || !floating) {
          setHovered(true);
        } else {
          showTimer.current = setTimeout(() => setHovered(true), SHOW_DELAY_MS);
        }
      },
      onPointerLeave: (event) => {
        clearTimeout(showTimer.current);
        clearTimeout(hideTimer.current);
        // A focused element removed from the bar, such as a closed tab's
        // close button, fires no blur, so focus is checked again here.
        if (!event.currentTarget.contains(document.activeElement)) {
          setFocused(false);
        }
        hideTimer.current = setTimeout(() => setHovered(false), HIDE_DELAY_MS);
      },
      onFocus: () => setFocused(true),
      onBlur: (event) => {
        // Focus moving within the bar is still focus in it.
        if (!(event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget))) {
          setFocused(false);
        }
      },
    },
  };
}
