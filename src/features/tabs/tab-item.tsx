import { internalTitle } from "@app/router";
import { type Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useId, useState } from "react";

interface TabItemProps {
  tab: Tab;
  active: boolean;
  pairPosition?: number;
  /** Opens the tab's menu at the pointer. */
  onMenu: (at: { x: number; y: number }) => void;
}

/**
 * One tab: a rounded rectangle that shrinks evenly with its neighbours.
 *
 * Close stays out of the way until it matters: it shows on the active tab and
 * on hover. A tab kept loaded carries a marker, so a tab holding a webview of
 * its own is recognisable at a glance; keeping it loaded is set from the site
 * menu, not here. So does a tab unloaded to free memory, which would otherwise
 * reload for no reason the user can see.
 *
 * Haku's own pages carry the Haku mark in the text's colour; a web page
 * carries its favicon, and one that crashed carries a warning in its place.
 */
export function TabItem({ tab, active, pairPosition = -1, onMenu }: TabItemProps) {
  const { private: isPrivate } = useBrowserState();
  const membershipId = useId();
  const visit = tab.history.entries[tab.history.index];
  const title = internalTitle(visit?.url ?? "") ?? (visit?.title?.trim() || t("tabs.untitled"));
  const discarded = tab.presence.status === "discarded";
  const state = { discarded: t("tabs.discarded"), frozen: t("tabs.frozen"), crashed: t("tabs.crashed") }[
    tab.presence.status as string
  ];

  return (
    <li className="flex min-w-10 flex-1">
      {/* The whole tab, close button included, answers a right-click; the
          keyboard reaches the same menu from the tab's button. */}
      <div
        onContextMenu={(event) => {
          event.preventDefault();
          // From the keyboard there is no pointer, and the event says 0, 0:
          // the menu opens below the tab instead.
          if (event.clientX === 0 && event.clientY === 0) {
            const rect = event.currentTarget.getBoundingClientRect();
            onMenu({ x: rect.left, y: rect.bottom });
          } else {
            onMenu({ x: event.clientX, y: event.clientY });
          }
        }}
        className={cx(
          "group flex h-(--control-height) w-full min-w-0 items-center gap-1 rounded-(--radius-control) pr-1 pl-2 transition-colors",
          pairPosition === 0 && "rounded-r-none",
          pairPosition === 1 && "rounded-l-none",
          active ? "bg-chrome-raised text-text" : "text-text-secondary hover:bg-text/6 hover:text-text",
          discarded && !active && "opacity-70",
        )}
        title={state ? `${title} — ${state}` : title}
      >
        <button
          type="button"
          data-tab-drag-handle
          className="flex min-w-0 flex-1 items-center gap-2 self-stretch text-left outline-offset-[-2px]"
          aria-current={active ? "page" : undefined}
          aria-describedby={pairPosition >= 0 ? membershipId : undefined}
          onClick={() => void commands.selectTab(tab.id)}
          onAuxClick={(event) => {
            // Middle click closes, as it does in every other browser.
            if (event.button === 1) {
              event.preventDefault();
              void commands.closeTab(tab.id);
            }
          }}
        >
          {tab.presence.status === "internal" ? (
            <i className="ic-haku ic-md shrink-0" aria-hidden="true" />
          ) : tab.presence.status === "crashed" ? (
            // Found in the strip without opening every tab.
            <i className="ic-triangle-alert ic-md text-destructive shrink-0" aria-hidden="true" />
          ) : (
            <Favicon key={visit?.favicon ?? ""} src={isPrivate ? null : (visit?.favicon ?? null)} />
          )}
          <span className="truncate">{title}</span>
        </button>

        {pairPosition >= 0 && (
          <span id={membershipId} className="sr-only">
            {t("split.membership")}: {t(pairPosition === 0 ? "split.left" : "split.right")}
          </span>
        )}
        {!active && tab.dialog !== null && (
          <button
            type="button"
            className="btn ghost min-w-0 shrink text-xs"
            aria-label={t("split.waitingTab").replace("{title}", () => title)}
            onClick={() => void commands.selectTab(tab.id)}
          >
            <span className="truncate">{t("split.waiting")}</span>
          </button>
        )}
        {tab.relieved && (
          <i
            className="ic-memory-stick ic-xs text-text-secondary shrink-0"
            title={t("tabs.relieved")}
            aria-label={t("tabs.relieved")}
          />
        )}
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
        className="btn icon ghost p-xs [--ui-radius:var(--radius-small)]"
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
 * image swaps in the fallback rather than leaving a gap in the strip. It is
 * keyed by its address, so every change of address, including back to one
 * that failed before, tries the image again.
 */
function Favicon({ src }: { src: string | null }) {
  const [broken, setBroken] = useState(false);

  if (!src || broken) {
    return <i className="ic-globe ic-md shrink-0" aria-hidden="true" />;
  }
  return <img src={src} alt="" draggable={false} className="size-4 shrink-0" onError={() => setBroken(true)} />;
}
