import { type SidebarSide } from "@bindings";
import { updateSettings } from "@ipc/store";
import { t } from "@shared/i18n";
import { useRef, type PointerEvent } from "react";

import { DEFAULT_SIDEBAR_WIDTH } from "./shell-layout";

/** Bounds of the tab column's width in CSS pixels, as Rust clamps it. */
export const MIN_SIDEBAR_WIDTH = 224;
export const MAX_SIDEBAR_WIDTH = 480;
/** How far one arrow key moves the edge. */
const KEY_STEP = 16;

/** The width the shell's grid and the floating column read. */
const WIDTH_PROPERTY = "--haku-sidebar-width";

export function clampSidebarWidth(width: number): number {
  return Math.round(Math.min(MAX_SIDEBAR_WIDTH, Math.max(MIN_SIDEBAR_WIDTH, width)));
}

/** A drag in progress: where it started, and the width it has reached. */
interface Drag {
  pointerId: number;
  x: number;
  width: number;
  current: number;
}

interface SidebarResizerProps {
  side: SidebarSide;
  width: number;
}

/**
 * The sidebar's inner edge, dragged to resize it.
 *
 * While dragging it writes the width straight to the shell's style, so the
 * page follows the pointer without a render or a settings write per frame;
 * the page is measured where it lands, as it is when the window resizes.
 * Releasing stores the width once. A double click restores the default.
 */
export function SidebarResizer({ side, width }: SidebarResizerProps) {
  const drag = useRef<Drag | null>(null);
  const handleRef = useRef<HTMLDivElement>(null);

  const shell = () => handleRef.current?.closest<HTMLElement>(".haku-shell") ?? null;
  const show = (next: number) => {
    shell()?.style.setProperty(WIDTH_PROPERTY, `${next}px`);
    handleRef.current?.setAttribute("aria-valuenow", String(next));
  };
  const store = async (next: number) => {
    if (next === width) {
      return;
    }
    const stored = await updateSettings((current) => ({ ...current, sidebarWidth: next }));
    // A failed write leaves the stored width; the edge goes back to it.
    if (stored === null) {
      show(width);
    }
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) {
      return;
    }
    const moved = event.clientX - current.x;
    current.current = clampSidebarWidth(current.width + (side === "left" ? moved : -moved));
    show(current.current);
  };
  const end = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) {
      return;
    }
    drag.current = null;
    void store(current.current);
  };

  return (
    <div
      ref={handleRef}
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- An interactive window splitter, not a thematic break.
      role="separator"
      tabIndex={0}
      aria-label={t("sidebar.resize")}
      aria-orientation="vertical"
      aria-valuemin={MIN_SIDEBAR_WIDTH}
      aria-valuemax={MAX_SIDEBAR_WIDTH}
      aria-valuenow={width}
      data-sidebar-resizer
      className="haku-sidebar-resizer"
      onPointerDown={(event) => {
        if (event.button !== 0 || !event.isPrimary) {
          return;
        }
        event.preventDefault();
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = {
          pointerId: event.pointerId,
          x: event.clientX,
          width,
          current: width,
        };
      }}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onLostPointerCapture={end}
      onDoubleClick={() => {
        show(DEFAULT_SIDEBAR_WIDTH);
        void store(DEFAULT_SIDEBAR_WIDTH);
      }}
      onKeyDown={(event) => {
        const grow = side === "left" ? "ArrowRight" : "ArrowLeft";
        const shrink = side === "left" ? "ArrowLeft" : "ArrowRight";
        const next =
          event.key === grow
            ? width + KEY_STEP
            : event.key === shrink
              ? width - KEY_STEP
              : event.key === "Home"
                ? MIN_SIDEBAR_WIDTH
                : event.key === "End"
                  ? MAX_SIDEBAR_WIDTH
                  : null;
        if (next === null) {
          return;
        }
        event.preventDefault();
        const clamped = clampSidebarWidth(next);
        show(clamped);
        void store(clamped);
      }}
    />
  );
}
