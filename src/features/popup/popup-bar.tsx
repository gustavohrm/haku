import { type Tab } from "@bindings";
import { connectionIcon, displayAddress } from "@features/toolbar/address";
import { WindowControls } from "@features/window-controls/window-controls";
import { useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";

interface PopupBarProps {
  tab: Tab | null;
}

/**
 * The one bar of a popup window: where its page is, and the window's controls.
 *
 * A popup has no tab strip and no address field. Its address is still shown,
 * read-only, so a page cannot open a window that passes for another site's
 * sign-in without saying where it really is.
 */
export function PopupBar({ tab }: PopupBarProps) {
  const { private: isPrivate } = useBrowserState();
  const url = tab ? (tab.history.entries[tab.history.index]?.url ?? "") : "";

  return (
    <header className="haku-bar flex h-8.5 shrink-0 items-center gap-2 pl-3" data-tauri-drag-region>
      <i className={cx(connectionIcon(url), "ic-sm text-text-secondary shrink-0")} aria-hidden="true" />
      <output
        className="text-text-secondary min-w-0 flex-1 truncate"
        aria-label={t("popup.address")}
        title={url}
        data-tauri-drag-region
      >
        {displayAddress(url)}
      </output>
      {isPrivate && (
        <span className="bg-chrome-raised text-text shrink-0 rounded-(--radius-control) px-2.5 py-1 font-medium">
          {t("private.label")}
        </span>
      )}
      <WindowControls />
    </header>
  );
}
