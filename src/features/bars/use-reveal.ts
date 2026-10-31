import { useEffect, useRef, useState, useSyncExternalStore, type FocusEvent, type PointerEvent } from "react";

import { barHoldStore, type BarContent } from "./holds";

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
 * Whether a floating bar is shown: while the pointer is on it or its edge,
 * while anything in it has focus, while something opened from it holds it,
 * or while something in it needs attention.
 * [Appearance § Bars](../../../docs/specs/appearance.md#bars).
 *
 * @param contents - What the bar holds, so holds on it count.
 * @param isNeeded - Something in it needs attention without a pointer.
 */
export function useReveal(contents: readonly BarContent[], isNeeded: boolean): Reveal {
  const [isHovered, setHovered] = useState(false);
  const [isFocused, setFocused] = useState(false);
  const holds = useSyncExternalStore(barHoldStore.subscribe, barHoldStore.get);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => () => clearTimeout(hideTimer.current), []);

  const isHeld = contents.some((content) => holds[content] > 0);
  return {
    revealed: isHovered || isFocused || isHeld || isNeeded,
    handlers: {
      onPointerEnter: () => {
        clearTimeout(hideTimer.current);
        setHovered(true);
      },
      onPointerLeave: (event) => {
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
