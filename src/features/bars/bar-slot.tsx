import { useOverlay } from "@features/overlays/use-overlay";
import { cx } from "@shared/class-names";
import { useRef, type ReactNode } from "react";

import { type BarContent } from "./holds";
import { useReveal } from "./use-reveal";

interface BarSlotProps {
  /** The window edge the bar belongs to: the top, or the left side for the tab column. */
  edge: "top" | "side";
  /** What the bar holds, so a hold on it shows the bar. */
  contents: readonly BarContent[];
  floating: boolean;
  /** Something in the bar needs attention, such as a waiting dialog. */
  isNeeded: boolean;
  children: ReactNode;
}

/**
 * Where a bar goes: in the shell's grid while docked, or over the page while
 * floating, shown as [Appearance § Bars](../../../docs/specs/appearance.md#bars)
 * describes.
 *
 * A floating bar keeps a strip at its window edge, in the page's margin, which
 * is chrome rather than page: that is where the chrome sees the pointer
 * arrive. Hidden, the bar is clipped rather than removed, so it stays in the
 * tab order and focusing it, as Ctrl+L does, is what shows it. Shown, it is
 * opaque, square and edged on its page side, and registered as an overlay so
 * it takes input over the page.
 */
export function BarSlot({ edge, contents, floating, isNeeded, children }: BarSlotProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const { revealed, handlers } = useReveal(contents, isNeeded);
  useOverlay(panelRef, floating && revealed);
  const isTop = edge === "top";

  // Docked and floating render the same tree, so switching between them
  // restyles the bar rather than remounting it: typed address text, focus and
  // the column's scroll position survive the switch.
  return (
    // Floating, it takes no input itself: over an internal page, which the
    // chrome draws, anything outside the strip and the shown bar must reach
    // the page.
    <div
      className={cx(
        "haku-bar-slot min-h-0 min-w-0",
        floating
          ? cx("pointer-events-none absolute left-0 z-1", isTop ? "top-0 right-0" : "top-0 bottom-0 w-60")
          : "flex flex-col",
      )}
      data-edge={edge}
      data-revealed={floating ? revealed : undefined}
      {...(floating ? handlers : {})}
    >
      {floating && (
        <div
          aria-hidden="true"
          data-bar-edge
          className={cx("pointer-events-auto absolute top-0 left-0", isTop ? "right-0 h-1" : "bottom-0 w-1")}
        />
      )}
      <div
        ref={panelRef}
        className={cx(
          "flex min-h-0 min-w-0 flex-col",
          !floating && "flex-1",
          floating && "bg-chrome border-text/15",
          floating && (isTop ? "relative border-b" : "absolute top-8.5 bottom-0 left-0 w-60 border-t border-r"),
          floating && (revealed ? "pointer-events-auto" : "pointer-events-none [clip-path:inset(0_0_100%_0)]"),
        )}
      >
        {children}
      </div>
    </div>
  );
}
