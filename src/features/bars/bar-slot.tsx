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
export function BarSlot(props: BarSlotProps) {
  if (!props.floating) {
    return (
      <div className="haku-bar-slot flex min-h-0 min-w-0 flex-col" data-edge={props.edge}>
        {props.children}
      </div>
    );
  }
  return <FloatingBar {...props} />;
}

function FloatingBar({ edge, contents, isNeeded, children }: BarSlotProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const { revealed, handlers } = useReveal(contents, isNeeded);
  useOverlay(panelRef, revealed);
  const isTop = edge === "top";

  return (
    // Takes no input itself: over an internal page, which the chrome draws,
    // anything outside the strip and the shown bar must reach the page.
    <div
      className={cx(
        "haku-bar-slot pointer-events-none absolute left-0 z-1",
        isTop ? "top-0 right-0" : "top-0 bottom-0 w-60",
      )}
      data-edge={edge}
      data-revealed={revealed}
      {...handlers}
    >
      <div
        aria-hidden="true"
        data-bar-edge
        className={cx("pointer-events-auto absolute top-0 left-0", isTop ? "right-0 h-1" : "bottom-0 w-1")}
      />
      <div
        ref={panelRef}
        className={cx(
          "bg-chrome border-text/15 flex min-h-0 flex-col",
          isTop ? "relative border-b" : "absolute top-8.5 bottom-0 left-0 w-60 border-t border-r",
          revealed ? "pointer-events-auto" : "pointer-events-none [clip-path:inset(0_0_100%_0)]",
        )}
      >
        {children}
      </div>
    </div>
  );
}
