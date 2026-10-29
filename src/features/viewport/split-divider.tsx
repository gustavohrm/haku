import type { TabId } from "@bindings";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";
import { useEffect, useRef, useState, type RefObject } from "react";

export const DIVIDER_WIDTH = 4;
const MIN_PANE_WIDTH = 240;
const RATIO_STEP = 0.05;

/** Rendered limits preserve both panes even when the window is too narrow. */
export function splitLimits(width: number): { min: number; max: number } {
  if (width < MIN_PANE_WIDTH * 2 + DIVIDER_WIDTH) {
    return { min: 0.5, max: 0.5 };
  }
  const min = Math.max(0.2, MIN_PANE_WIDTH / (width - DIVIDER_WIDTH));
  return { min, max: 1 - min };
}

export function clampSplitRatio(ratio: number, width: number): number {
  const { min, max } = splitLimits(width);
  return Math.min(max, Math.max(min, ratio));
}

interface SplitDividerProps {
  containerRef: RefObject<HTMLDivElement | null>;
  ratio: number;
  tabId: TabId;
  revision: number;
  assignments: string;
  names: [string, string];
}

/** Temporary drag geometry lives in refs; Rust receives one intent on release. */
export function SplitDivider({ containerRef, ratio, tabId, revision, assignments, names }: SplitDividerProps) {
  const separatorRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ pointer: number; ratio: number } | null>(null);
  const renderedRatio = useRef(ratio);
  const [width, setWidth] = useState(0);
  const { min, max } = splitLimits(width);
  const isDisabled = min === max;

  useEffect(() => {
    const container = containerRef.current;
    const separator = separatorRef.current;
    if (!container || !separator) {
      return;
    }
    const apply = () => {
      const nextWidth = container.getBoundingClientRect().width;
      const current = drag.current;
      drag.current = null;
      if (current && separator.hasPointerCapture?.(current.pointer)) {
        separator.releasePointerCapture(current.pointer);
      }
      setWidth(nextWidth);
      renderedRatio.current = clampSplitRatio(ratio, nextWidth);
      container.style.setProperty("--split-ratio", String(renderedRatio.current));
      separator.setAttribute("aria-valuenow", String(Math.round(renderedRatio.current * 100)));
    };
    apply();
    const observer = new ResizeObserver(apply);
    observer.observe(container);
    return () => {
      observer.disconnect();
      const current = drag.current;
      drag.current = null;
      if (current && separator.hasPointerCapture?.(current.pointer)) {
        separator.releasePointerCapture(current.pointer);
      }
      container.style.setProperty(
        "--split-ratio",
        String(clampSplitRatio(ratio, container.getBoundingClientRect().width)),
      );
    };
  }, [containerRef, ratio, revision, assignments]);

  const applyRatio = (next: number) => {
    renderedRatio.current = clampSplitRatio(next, containerRef.current?.getBoundingClientRect().width ?? 0);
    containerRef.current?.style.setProperty("--split-ratio", String(renderedRatio.current));
    separatorRef.current?.setAttribute("aria-valuenow", String(Math.round(renderedRatio.current * 100)));
  };
  const cancel = () => {
    const current = drag.current;
    if (!current) {
      return;
    }
    drag.current = null;
    applyRatio(current.ratio);
    if (separatorRef.current?.hasPointerCapture?.(current.pointer)) {
      separatorRef.current.releasePointerCapture(current.pointer);
    }
  };
  return (
    <div
      ref={separatorRef}
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- This is an interactive window splitter, not a thematic break.
      role="separator"
      tabIndex={0}
      aria-label={t("split.divider")
        .replace("{left}", () => names[0])
        .replace("{right}", () => names[1])}
      aria-orientation="vertical"
      aria-valuemin={Math.round(min * 100)}
      aria-valuemax={Math.round(max * 100)}
      aria-valuenow={Math.round(clampSplitRatio(ratio, width) * 100)}
      aria-disabled={isDisabled || undefined}
      className="bg-chrome focus-visible:bg-text-secondary relative w-[4px] cursor-col-resize touch-none outline-offset-[-2px] aria-disabled:cursor-default"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          cancel();
          event.preventDefault();
          return;
        }
        if (isDisabled || drag.current) {
          return;
        }
        const next = {
          ArrowLeft: renderedRatio.current - RATIO_STEP,
          ArrowRight: renderedRatio.current + RATIO_STEP,
          Home: min,
          End: max,
          Enter: 0.5,
        }[event.key];
        if (next === undefined) {
          return;
        }
        event.preventDefault();
        void commands.setSplitRatio(tabId, clampSplitRatio(next, width));
      }}
      onPointerDown={(event) => {
        if (event.button !== 0 || isDisabled || drag.current) {
          return;
        }
        event.preventDefault();
        event.currentTarget.focus();
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = { pointer: event.pointerId, ratio: renderedRatio.current };
      }}
      onPointerMove={(event) => {
        if (!drag.current || drag.current.pointer !== event.pointerId) {
          return;
        }
        const rect = containerRef.current!.getBoundingClientRect();
        applyRatio((event.clientX - rect.left) / (rect.width - DIVIDER_WIDTH));
      }}
      onPointerUp={(event) => {
        if (!drag.current || drag.current.pointer !== event.pointerId) {
          return;
        }
        drag.current = null;
        event.currentTarget.releasePointerCapture(event.pointerId);
        void commands.setSplitRatio(tabId, renderedRatio.current);
      }}
      onPointerCancel={cancel}
      onLostPointerCapture={cancel}
    />
  );
}
