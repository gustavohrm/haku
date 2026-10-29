import { type Tab } from "@bindings";
import { stripMenu, tabMenu } from "@features/context-menu/items";
import { openMenu } from "@features/context-menu/store";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { t } from "@shared/i18n";
import { useEffect, useRef } from "react";

import { TabItem } from "./tab-item";

/**
 * The tab bar.
 *
 * Ordering comes from Rust and is rendered as given. Reordering is a command
 * rather than local state, so the strip has nothing to keep in sync.
 *
 * The new-tab button follows the last tab instead of sitting at the far end,
 * and whatever width the tabs leave free stays a drag region for the window.
 */
export function TabStrip() {
  const { tabs, active, splits } = useBrowserState();
  const drag = useRef<{ tab: number; from: number; size: number; gap: number } | null>(null);
  const indicator = useRef<HTMLLIElement>(null);
  const signature = tabs.map((tab) => tab.id).join(",") + JSON.stringify(splits.map((split) => split.tabs));
  const clearDrag = () => {
    drag.current = null;
    if (indicator.current) {
      indicator.current.hidden = true;
      delete indicator.current.dataset.dropGap;
    }
  };
  useEffect(() => clearDrag, [signature]);
  const units = tabs
    .filter((tab) => !splits.some((split) => split.tabs[1] === tab.id))
    .map((tab) => {
      const pair = splits.find((split) => split.tabs[0] === tab.id);
      return pair ? pair.tabs.map((id) => tabs.find((candidate) => candidate.id === id)!) : [tab];
    });

  const openTabMenu = (tab: Tab, at: { x: number; y: number }) =>
    openMenu({
      at,
      label: t("menu.tab"),
      items: tabMenu(
        tab,
        tabs,
        {
          reload: (id) => void commands.reloadTab(id),
          duplicate: (url) => void commands.openTab(url, true),
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
      className="flex min-w-0 flex-1 items-center gap-1.5"
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
          items: stripMenu({
            newTab: () => void commands.openTab(null, true),
            reopen: () => void commands.reopenClosedTab(),
          }),
        });
      }}
    >
      <ul className="relative flex min-w-0 shrink items-center gap-0.5" aria-label={t("tabs.list")}>
        {units.map((unit) => {
          const first = unit[0]!;
          const from = tabs.findIndex((tab) => tab.id === first.id);
          return (
            // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Native drag moves this item; buttons retain keyboard selection and menus.
            <li
              key={first.id}
              data-tab-unit
              draggable
              className={
                unit.length === 2
                  ? "bg-text/6 flex w-100 min-w-20 shrink rounded-(--radius-control)"
                  : "flex w-50 min-w-10 shrink"
              }
              onDragStart={(event) => {
                drag.current = { tab: first.id, from, size: unit.length, gap: from };
                event.dataTransfer.effectAllowed = "move";
                event.dataTransfer.setData("text/plain", String(first.id));
              }}
              onDragOver={(event) => {
                if (!drag.current) {
                  return;
                }
                event.preventDefault();
                const rect = event.currentTarget.getBoundingClientRect();
                const isAfter = event.clientX >= rect.left + rect.width / 2;
                const gap = from + (isAfter ? unit.length : 0);
                drag.current.gap = gap;
                if (indicator.current) {
                  indicator.current.hidden = false;
                  indicator.current.dataset.dropGap = String(gap);
                  indicator.current.style.left = `${(isAfter ? rect.right : rect.left) - event.currentTarget.parentElement!.getBoundingClientRect().left}px`;
                }
              }}
              onDrop={(event) => {
                const current = drag.current;
                if (!current) {
                  return;
                }
                event.preventDefault();
                const to =
                  current.gap > current.from ? Math.max(current.from, current.gap - current.size) : current.gap;
                clearDrag();
                if (to !== current.from) {
                  void commands.reorderTab(current.tab, to);
                }
              }}
              onDragEnd={clearDrag}
            >
              <ul className="flex min-w-0 flex-1" aria-label={unit.length === 2 ? t("split.membership") : undefined}>
                {unit.map((tab, position) => (
                  <TabItem
                    key={tab.id}
                    tab={tab}
                    active={tab.id === active}
                    pairPosition={unit.length === 2 ? position : -1}
                    onMenu={(at) => openTabMenu(tab, at)}
                  />
                ))}
              </ul>
            </li>
          );
        })}
        <li
          ref={indicator}
          hidden
          aria-hidden="true"
          className="bg-accent pointer-events-none absolute inset-y-0 w-0.5"
        />
      </ul>
      <button
        type="button"
        className="btn icon ghost shrink-0"
        aria-label={t("tabs.new")}
        title={t("tabs.new")}
        onClick={() => void commands.openTab(null, true)}
      >
        <i className="ic-plus ic-sm" aria-hidden="true" />
      </button>
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
