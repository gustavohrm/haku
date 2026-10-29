import { type Tab } from "@bindings";
import { stripMenu, tabMenu } from "@features/context-menu/items";
import { openMenu } from "@features/context-menu/store";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { t } from "@shared/i18n";

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
  const { tabs, active, viewports } = useBrowserState();

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
          closePane: (id) => void commands.closePane(id),
        },
        viewports,
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
      <ul className="flex min-w-0 shrink items-center gap-0.5" aria-label={t("tabs.list")}>
        {tabs.map((tab: Tab) => (
          <TabItem
            key={tab.id}
            tab={tab}
            active={tab.id === active}
            panePosition={viewports.length === 2 ? viewports.findIndex((pane) => pane.tab === tab.id) : -1}
            onMenu={(at) => openTabMenu(tab, at)}
          />
        ))}
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
