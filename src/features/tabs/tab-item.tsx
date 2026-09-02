import { commands, type Tab } from "@bindings";
import { t } from "@shared/i18n";
import { useState } from "react";

interface TabItemProps {
  tab: Tab;
  active: boolean;
}

export function TabItem({ tab, active }: TabItemProps) {
  const visit = tab.history.entries[tab.history.index];
  const title = visit?.title?.trim() || t("tabs.untitled");
  const suspended = tab.presence.status === "suspended";

  return (
    <li className="min-w-0">
      <div
        className={`haku-tab ${active ? "haku-tab-active" : ""} ${suspended ? "haku-tab-suspended" : ""}`}
        title={suspended ? `${title} — ${t("tabs.suspended")}` : title}
      >
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
          aria-current={active ? "page" : undefined}
          onClick={() => void commands.selectTab(tab.id)}
          onAuxClick={(event) => {
            // Middle click closes, as it does in every other browser.
            if (event.button === 1) {
              event.preventDefault();
              void commands.closeTab(tab.id);
            }
          }}
        >
          <Favicon src={visit?.favicon ?? null} />
          <span className="truncate">{title}</span>
        </button>

        <button
          type="button"
          className="haku-tab-action"
          aria-label={tab.fixed ? t("tabs.unpin") : t("tabs.pin")}
          title={tab.fixed ? t("tabs.unpin") : t("tabs.pin")}
          aria-pressed={tab.fixed}
          onClick={() => void commands.setTabFixed(tab.id, !tab.fixed)}
        >
          <i className={tab.fixed ? "ic-bookmark-check" : "ic-bookmark"} aria-hidden="true" />
        </button>

        <button
          type="button"
          className="haku-tab-action"
          aria-label={t("tabs.close")}
          title={t("tabs.close")}
          onClick={() => void commands.closeTab(tab.id)}
        >
          <i className="ic-x" aria-hidden="true" />
        </button>
      </div>
    </li>
  );
}

/**
 * A page's icon, falling back to a generic one.
 *
 * Favicons come from arbitrary sites and frequently fail to load, so a broken
 * image swaps in the fallback rather than leaving a gap in the strip.
 */
function Favicon({ src }: { src: string | null }) {
  const [broken, setBroken] = useState(false);

  if (!src || broken) {
    return <i className="ic-globe haku-favicon" aria-hidden="true" />;
  }
  return <img src={src} alt="" className="haku-favicon" onError={() => setBroken(true)} />;
}
