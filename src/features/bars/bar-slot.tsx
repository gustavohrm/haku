import { useOverlay } from "@features/overlays/use-overlay";
import { useRef, useState, type ReactNode } from "react";

import { type BarContent } from "./holds";
import { useReveal } from "./use-reveal";

/** The stacking order the next bar to show takes, above every bar shown before it. */
let nextLayer = 1;

interface BarSlotProps {
  /** The window edge the bar belongs to: the top, or the sidebar's side. */
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
 * describes. Its geometry is in `src/styles/index.css`, read from the shell's
 * layout attributes, so mirroring the sidebar is a stylesheet concern.
 *
 * A floating bar keeps a strip at its window edge, in the page's margin, which
 * is chrome rather than page: that is where the chrome sees the pointer
 * arrive. Hidden, the bar is moved off its edge rather than removed, so it
 * stays in the tab order and focusing it, as Ctrl+L does, is what shows it.
 * Shown, it slides in as a rounded, opaque card edged all round, registered as
 * an overlay that follows it while it moves, so it takes input over the page.
 * Floating bars can overlap where the column meets the toolbar; the one shown
 * last is on top, so the bar the pointer just reached for is the one it gets.
 */
export function BarSlot({ edge, contents, floating, isNeeded, children }: BarSlotProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const { revealed, handlers } = useReveal(contents, isNeeded);
  const isShown = floating && revealed;
  useOverlay(panelRef, isShown);
  // Taken as it shows, during render rather than in an effect, so it is on
  // top from its first frame.
  const [layer, setLayer] = useState(0);
  const [wasShown, setWasShown] = useState(false);
  if (isShown !== wasShown) {
    setWasShown(isShown);
    if (isShown) {
      setLayer(nextLayer++);
    }
  }

  // Docked and floating render the same tree, so switching between them
  // restyles the bar rather than remounting it: typed address text, focus and
  // the column's scroll position survive the switch.
  return (
    // Floating, it takes no input itself: over an internal page, which the
    // chrome draws, anything outside the strip and the shown bar must reach
    // the page.
    <div
      className="haku-bar-slot"
      data-edge={edge}
      data-floating={floating}
      data-revealed={floating ? revealed : undefined}
      // Kept while it leaves, so a bar sliding away stays where it was.
      style={floating ? { zIndex: layer } : undefined}
      // Tracked while docked too, so a bar in use stays shown when it starts
      // floating.
      {...handlers}
    >
      {floating && <div aria-hidden="true" data-bar-edge className="haku-bar-edge" />}
      <div ref={panelRef} className="haku-bar-panel">
        {children}
      </div>
    </div>
  );
}
