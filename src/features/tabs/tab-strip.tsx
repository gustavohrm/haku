import { commands, type Tab, type TabId } from "@bindings";
import { useBrowserState } from "@ipc/hooks";
import { t } from "@shared/i18n";

import { TabItem } from "./tab-item";

/**
 * The tab bar.
 *
 * Ordering comes from Rust and is rendered as given. Reordering is a command
 * rather than local state, so the strip has nothing to keep in sync.
 */
export function TabStrip() {
  const { tabs, active } = useBrowserState();

  return (
    <div className="flex min-w-0 flex-1 items-end gap-1" data-tauri-drag-region>
      <ul className="flex min-w-0 flex-1 items-end gap-1" aria-label={t("tabs.new")}>
        {tabs.map((tab: Tab) => (
          <TabItem key={tab.id} tab={tab} active={tab.id === active} />
        ))}
      </ul>
      <button
        type="button"
        className="haku-icon-button shrink-0"
        aria-label={t("tabs.new")}
        title={t("tabs.new")}
        onClick={() => void commands.openTab(null, true)}
      >
        <i className="ic-plus" aria-hidden="true" />
      </button>
    </div>
  );
}

export function selectTab(id: TabId): void {
  void commands.selectTab(id);
}
