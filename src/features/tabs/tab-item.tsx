import { internalTitle } from "@app/router";
import { type Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useState } from "react";

interface TabItemProps {
  tab: Tab;
  active: boolean;
}

/**
 * One tab: a rounded rectangle that shrinks evenly with its neighbours.
 *
 * Close stays out of the way until it matters: it shows on the active tab and
 * on hover. A tab kept loaded carries a marker, so a tab holding a webview of
 * its own is recognisable at a glance; keeping it loaded is set from the site
 * menu, not here.
 */
export function TabItem({ tab, active }: TabItemProps) {
  const visit = tab.history.entries[tab.history.index];
  const title = internalTitle(visit?.url ?? "") ?? (visit?.title?.trim() || t("tabs.untitled"));
  const discarded = tab.presence.status === "discarded";
  const state = { discarded: t("tabs.discarded"), frozen: t("tabs.frozen") }[tab.presence.status as string];

  return (
    <li className="flex w-50 min-w-10 shrink">
      <div
        className={cx(
          "group flex h-(--control-height) w-full min-w-0 items-center gap-1 rounded-(--radius-control) pr-1 pl-2 transition-colors",
          active ? "bg-chrome-raised text-text" : "text-text-secondary hover:bg-text/6 hover:text-text",
          discarded && !active && "opacity-70",
        )}
        title={state ? `${title} — ${state}` : title}
      >
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-2 self-stretch text-left outline-none"
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

        {tab.fixed && (
          <i
            className="ic-pin ic-xs text-text-secondary shrink-0"
            title={t("tabs.fixed")}
            aria-label={t("tabs.fixed")}
          />
        )}
        <TabAction
          icon="ic-x"
          label={t("tabs.close")}
          alwaysVisible={active}
          onClick={() => void commands.closeTab(tab.id)}
        />
      </div>
    </li>
  );
}

interface TabActionProps {
  icon: string;
  label: string;
  /** Otherwise shown only while the tab is hovered or holds focus. */
  alwaysVisible: boolean;
  onClick: () => void;
}

function TabAction({ icon, label, alwaysVisible, onClick }: TabActionProps) {
  return (
    // Hidden on a wrapper rather than the button: `.btn` sets its own display,
    // and two utilities setting the same property on one element resolve by
    // Tailwind's sort order rather than by intent.
    <span className={cx("shrink-0", !alwaysVisible && "hidden group-focus-within:flex group-hover:flex")}>
      <button
        type="button"
        className="btn icon ghost dense [--ui-radius:var(--radius-small)]"
        aria-label={label}
        title={label}
        onClick={onClick}
      >
        <i className={cx(icon, "ic-xs")} aria-hidden="true" />
      </button>
    </span>
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
    return <i className="ic-globe ic-md shrink-0" aria-hidden="true" />;
  }
  return <img src={src} alt="" className="size-4 shrink-0" onError={() => setBroken(true)} />;
}
