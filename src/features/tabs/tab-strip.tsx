import { type Tab, type TabId } from "@bindings";
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
  const { tabs, active } = useBrowserState();

  return (
    <div className="flex min-w-0 flex-1 items-center gap-1.5" data-tauri-drag-region>
      <ul className="flex min-w-0 shrink items-center gap-0.5" aria-label={t("tabs.list")}>
        {tabs.map((tab: Tab) => (
          <TabItem key={tab.id} tab={tab} active={tab.id === active} />
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

export function selectTab(id: TabId): void {
  void commands.selectTab(id);
}
