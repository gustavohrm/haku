import { type Tab, type TabLayout } from "@bindings";
import { holdBar } from "@features/bars/holds";
import { stripMenu, tabMenu } from "@features/context-menu/items";
import { openMenu } from "@features/context-menu/store";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { Fragment, useEffect, useLayoutEffect, useRef } from "react";

import { TabItem } from "./tab-item";

const DRAG_THRESHOLD = 5;
/** How long a dropped tab's place is held while Rust answers with the new order. */
const SETTLE_TIMEOUT = 400;

interface TabDrag {
  tab: number;
  from: number;
  size: number;
  gap: number | null;
  /** How far the dragged unit sits from its place once dropped in the gap. */
  settle: number;
  /** Where each unit rested when the drag began, and how far the strip had scrolled. */
  rests?: { scroll: number; units: { start: number; end: number; size: number }[] };
  pointerId: number;
  target: HTMLElement;
  x: number;
  y: number;
  started: boolean;
}

/** The edges of a box along the strip's axis, and across it. */
function along(rect: DOMRect, isVertical: boolean) {
  return isVertical
    ? { start: rect.top, end: rect.bottom, size: rect.height, crossStart: rect.left, crossEnd: rect.right }
    : { start: rect.left, end: rect.right, size: rect.width, crossStart: rect.top, crossEnd: rect.bottom };
}

/**
 * The tab bar: a strip across the top, or a column down the side.
 *
 * Ordering comes from Rust and is rendered as given. Reordering is a command
 * rather than local state, so the strip has nothing to keep in sync. Both
 * layouts are this one component on another axis, so the column cannot lose
 * something the strip does ([Appearance § Tabs](../../../docs/specs/appearance.md#tabs)).
 *
 * The new-tab button follows the last tab instead of sitting at the far end,
 * and whatever space the tabs leave free stays a drag region for the window.
 * A menu opened here holds a floating bar on screen until it closes.
 *
 * A dragged tab follows the pointer and the others slide out of its way, so
 * the strip shows the order a drop would leave. That is drawn with `translate`
 * straight on the elements: the order itself stays Rust's, and a drop keeps
 * the picture in place until the new order arrives, so nothing jumps back.
 */
export function TabStrip({ layout = "horizontal" }: { layout?: TabLayout }) {
  const isVertical = layout === "vertical";
  const { tabs, active, splits } = useBrowserState();
  const drag = useRef<TabDrag | null>(null);
  const strip = useRef<HTMLUListElement>(null);
  const suppressClick = useRef(false);
  const indicator = useRef<HTMLLIElement>(null);
  /** How far each unit is drawn from its place, by its index in the strip. */
  const offsets = useRef<number[]>([]);
  const settling = useRef<number | undefined>(undefined);
  const signature = tabs.map((tab) => tab.id).join(",") + JSON.stringify(splits.map((split) => split.tabs));
  const place = (next: number[]) => {
    const list = strip.current;
    if (!list) {
      return;
    }
    for (const [index, offset] of next.entries()) {
      const unit = list.children[index] as HTMLElement | undefined;
      if (unit?.dataset.tabUnit !== undefined) {
        unit.style.translate = offset === 0 ? "" : isVertical ? `0 ${offset}px` : `${offset}px 0`;
      }
    }
    offsets.current = next;
  };
  const clearPicture = () => {
    window.clearTimeout(settling.current);
    settling.current = undefined;
    const list = strip.current;
    if (!list) {
      return;
    }
    delete list.dataset.reordering;
    for (const unit of list.children) {
      delete (unit as HTMLElement).dataset.dragging;
      (unit as HTMLElement).style.translate = "";
    }
    offsets.current = [];
  };
  const clearDrag = (keepPicture = false) => {
    const current = drag.current;
    drag.current = null;
    if (current) {
      suppressClick.current = false;
    }
    if (current?.target.hasPointerCapture(current.pointerId)) {
      current.target.releasePointerCapture(current.pointerId);
    }
    if (indicator.current) {
      indicator.current.hidden = true;
      delete indicator.current.dataset.dropGap;
    }
    if (!keepPicture) {
      clearPicture();
    }
  };
  // Before paint, so a new order is never drawn with the old one's offsets.
  useLayoutEffect(() => () => clearDrag(), [signature]);
  useEffect(() => {
    const cancel = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        clearDrag();
      }
    };
    window.addEventListener("keydown", cancel);
    return () => window.removeEventListener("keydown", cancel);
  }, []);
  const units = tabs
    .filter((tab) => !splits.some((split) => split.tabs[1] === tab.id))
    .map((tab) => {
      const pair = splits.find((split) => split.tabs[0] === tab.id);
      return pair ? pair.tabs.map((id) => tabs.find((candidate) => candidate.id === id)!) : [tab];
    });

  const updateDropTarget = (x: number, y: number) => {
    const current = drag.current;
    if (!current || !strip.current || !indicator.current) {
      return;
    }
    const [position, across] = isVertical ? [y, x] : [x, y];
    const bounds = along(strip.current.getBoundingClientRect(), isVertical);
    if (across < bounds.crossStart || across > bounds.crossEnd) {
      current.gap = null;
      current.settle = 0;
      indicator.current.hidden = true;
      delete indicator.current.dataset.dropGap;
      // Dropping here does nothing, and the strip says so by going back.
      place(offsets.current.map(() => 0));
      return;
    }
    // Where each unit rests, measured once before any is drawn elsewhere, and
    // carried along as the column scrolls.
    const children = strip.current.children;
    const scroll = isVertical ? strip.current.scrollTop : 0;
    current.rests ??= {
      scroll,
      units: units.map((_, index) => along(children[index]!.getBoundingClientRect(), isVertical)),
    };
    const scrolled = scroll - current.rests.scroll;
    const rests = current.rests.units.map((rect) => ({
      start: rect.start - scrolled,
      end: rect.end - scrolled,
      size: rect.size,
    }));
    let gap = 0;
    let slot = units.length;
    let edge = bounds.start;
    for (const [index, unit] of units.entries()) {
      const rect = rests[index]!;
      if (position < rect.start + rect.size / 2) {
        edge = rect.start;
        slot = index;
        break;
      }
      gap += unit.length;
      edge = rect.end;
    }
    current.gap = gap;
    const moved = units.findIndex((unit) => unit[0]!.id === current.tab);
    const own = rests[moved];
    if (own) {
      const to = slot > moved ? slot - 1 : slot;
      const after = rests[moved + 1];
      const before = rests[moved - 1];
      const step = own.size + (after ? after.start - own.end : before ? own.start - before.end : 0);
      // The unit sits in the column's scrolled content, so a scroll carries it too.
      const travel = position - (isVertical ? current.y : current.x) + scrolled;
      const landing = rests[to]!;
      current.settle = to > moved ? landing.end - own.end : to < moved ? landing.start - own.start : 0;
      strip.current.dataset.reordering = "";
      (children[moved] as HTMLElement).dataset.dragging = "";
      place(
        units.map((_, index) =>
          index === moved
            ? Math.min(Math.max(travel, bounds.start - own.start), bounds.end - own.end)
            : index > moved && index <= to
              ? -step
              : index < moved && index >= to
                ? step
                : 0,
        ),
      );
    }
    indicator.current.hidden = false;
    indicator.current.dataset.dropGap = String(gap);
    // The column scrolls, and the line sits in its scrolled content.
    if (isVertical) {
      indicator.current.style.top = `${edge - bounds.start + strip.current.scrollTop}px`;
    } else {
      indicator.current.style.left = `${edge - bounds.start}px`;
    }
  };

  const openTabMenu = (tab: Tab, at: { x: number; y: number }) =>
    openMenu({
      at,
      label: t("menu.tab"),
      onClose: holdBar("tabs"),
      items: tabMenu(
        tab,
        tabs,
        {
          reload: (id) => void commands.reloadTab(id),
          duplicate: (url) => void commands.openTab(url, true, "other"),
          setFixed: (id, fixed) => void commands.setTabFixed(id, fixed),
          close: (ids) => void closeTabs(ids),
          split: (id) => void commands.splitTab(id),
          separate: (id) => void commands.separateTabs(id),
        },
        { active, splits },
      ),
    });

  return (
    <div
      className={
        isVertical
          ? "flex min-h-0 flex-1 flex-col gap-(--haku-gap)"
          : "flex min-w-0 flex-1 items-center gap-(--haku-gap)"
      }
      data-tauri-drag-region
      onContextMenu={(event) => {
        // A tab answers with its own menu first.
        if (event.defaultPrevented) {
          return;
        }
        event.preventDefault();
        openMenu({
          at: { x: event.clientX, y: event.clientY },
          label: t("menu.tabs"),
          onClose: holdBar("tabs"),
          items: stripMenu({
            newTab: () => void commands.openTab(null, true, "other"),
            reopen: () => void commands.reopenClosedTab(),
          }),
        });
      }}
    >
      <ul
        ref={strip}
        className={cx(
          "relative flex gap-(--haku-gap)",
          isVertical ? "min-h-0 flex-col overflow-y-auto" : "min-w-0 shrink items-center",
        )}
        aria-label={t("tabs.list")}
        onClickCapture={(event) => {
          if (suppressClick.current && event.detail !== 0) {
            event.preventDefault();
            event.stopPropagation();
          }
          suppressClick.current = false;
        }}
      >
        {units.map((unit) => {
          const first = unit[0]!;
          const from = tabs.findIndex((tab) => tab.id === first.id);
          return (
            // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Pointer dragging moves the unit; its buttons retain selection and menus.
            <li
              key={first.id}
              data-tab-unit
              className={cx(
                "flex touch-none select-none",
                isVertical ? "shrink-0" : "shrink",
                !isVertical && (unit.length === 2 ? "w-100 min-w-22" : "w-50 min-w-10"),
              )}
              onPointerDown={(event) => {
                // While a dropped unit waits for the new order, the units are
                // drawn away from where the old order says they rest.
                if (event.button !== 0 || !event.isPrimary || drag.current || settling.current !== undefined) {
                  return;
                }
                suppressClick.current = false;
                const target = (event.target as HTMLElement).closest<HTMLElement>("[data-tab-drag-handle]");
                if (!target) {
                  return;
                }
                drag.current = {
                  tab: first.id,
                  from,
                  size: unit.length,
                  gap: null,
                  settle: 0,
                  pointerId: event.pointerId,
                  target,
                  x: event.clientX,
                  y: event.clientY,
                  started: false,
                };
                // Capture on the selection button so a below-threshold click still selects it.
                target.setPointerCapture(event.pointerId);
              }}
              onPointerMove={(event) => {
                const current = drag.current;
                if (!current || current.pointerId !== event.pointerId) {
                  return;
                }
                if (
                  !current.started &&
                  Math.hypot(event.clientX - current.x, event.clientY - current.y) < DRAG_THRESHOLD
                ) {
                  return;
                }
                current.started = true;
                suppressClick.current = true;
                event.preventDefault();
                updateDropTarget(event.clientX, event.clientY);
              }}
              onPointerUp={(event) => {
                const current = drag.current;
                if (!current || current.pointerId !== event.pointerId) {
                  return;
                }
                if (current.started) {
                  updateDropTarget(event.clientX, event.clientY);
                }
                const gap = current.gap;
                const to =
                  !current.started || gap === null
                    ? current.from
                    : gap > current.from
                      ? Math.max(current.from, gap - current.size)
                      : gap;
                const moves = to !== current.from;
                if (moves) {
                  // The unit slides into the gap and stays there until the new
                  // order arrives, or until it evidently will not.
                  const moved = units.findIndex((unit) => unit[0]!.id === current.tab);
                  delete (strip.current?.children[moved] as HTMLElement | undefined)?.dataset.dragging;
                  place(offsets.current.map((offset, index) => (index === moved ? current.settle : offset)));
                  window.clearTimeout(settling.current);
                  settling.current = window.setTimeout(clearPicture, SETTLE_TIMEOUT);
                }
                clearDrag(moves);
                suppressClick.current = current.started;
                if (moves) {
                  void commands.reorderTab(current.tab, to);
                }
              }}
              onPointerCancel={(event) => {
                if (drag.current?.pointerId === event.pointerId) {
                  clearDrag();
                }
              }}
              onLostPointerCapture={(event) => {
                if (drag.current?.pointerId === event.pointerId) {
                  clearDrag();
                }
              }}
            >
              <ul
                className={cx("flex min-w-0 flex-1", isVertical && "flex-col")}
                aria-label={unit.length === 2 ? t("split.membership") : undefined}
              >
                {unit.map((tab, position) => (
                  <Fragment key={tab.id}>
                    {position === 1 && (
                      // The link between the halves is the control that undoes
                      // it. It sits on the neck the two tabs are joined by, in
                      // the middle of the gap between them.
                      <li
                        className="haku-pair-seam"
                        data-axis={isVertical ? "column" : "row"}
                        data-first={first.id === active ? "active" : undefined}
                        data-second={tab.id === active ? "active" : undefined}
                      >
                        <button
                          type="button"
                          className="haku-pair-link group"
                          aria-label={t("split.separate")}
                          title={t("split.separate")}
                          onClick={() => void commands.separateTabs(first.id)}
                        >
                          {/* Hidden on wrappers: an icon sets its own display. */}
                          <span className="contents group-hover:hidden group-focus-visible:hidden">
                            <i className="ic-link ic-xs" aria-hidden="true" />
                          </span>
                          <span className="hidden group-hover:contents group-focus-visible:contents">
                            <i className="ic-unlink ic-xs" aria-hidden="true" />
                          </span>
                        </button>
                      </li>
                    )}
                    <TabItem
                      tab={tab}
                      active={tab.id === active}
                      pairPosition={unit.length === 2 ? position : -1}
                      layout={layout}
                      onMenu={(at) => openTabMenu(tab, at)}
                    />
                  </Fragment>
                ))}
              </ul>
            </li>
          );
        })}
        <li
          ref={indicator}
          hidden
          aria-hidden="true"
          className={cx("pointer-events-none absolute", isVertical ? "inset-x-0 top-0 h-0.5" : "inset-y-0 w-0.5")}
        />
      </ul>
      {isVertical ? (
        // A row like the tabs above it, so the column reads as one list.
        <button
          type="button"
          className="btn ghost text-text-secondary h-(--control-height) w-full shrink-0 justify-start gap-2 px-2 font-normal"
          onClick={() => void commands.openTab(null, true, "other")}
        >
          <i className="ic-plus ic-md" aria-hidden="true" />
          {t("tabs.new")}
        </button>
      ) : (
        <button
          type="button"
          className="btn icon ghost shrink-0"
          aria-label={t("tabs.new")}
          title={t("tabs.new")}
          onClick={() => void commands.openTab(null, true, "other")}
        >
          <i className="ic-plus ic-sm" aria-hidden="true" />
        </button>
      )}
    </div>
  );
}

/** Closes tabs one after another, as each close reshapes the strip. */
async function closeTabs(ids: readonly number[]) {
  for (const id of ids) {
    // In turn on purpose: each close moves the active tab and the slots.
    // oxlint-disable-next-line no-await-in-loop
    await commands.closeTab(id);
  }
}
