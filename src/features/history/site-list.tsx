import { type HistorySite } from "@bindings";
import { Favicon } from "@features/bookmarks/favicon";
import { openMenu } from "@features/context-menu/store";
import { t } from "@shared/i18n";

import { counted, countVisits, forLatestMenu, siteFilter, siteRuleFor } from "./history-actions";
import { siteItems } from "./row-menu";

interface SiteListProps {
  sites: HistorySite[];
  isPrivate: boolean;
  /** Opens the visits view narrowed to the site. */
  showSite: (host: string) => void;
}

/**
 * One row per site with its visit count and last visit, most visited first.
 * A site's row has a menu, from a right-click or its actions button, with the
 * items a visit row's menu ends with about its site.
 */
export function SiteList({ sites, isPrivate, showSite }: SiteListProps) {
  return (
    <ul className="flex flex-col">
      {sites.map((site) => (
        <SiteRow key={site.host} site={site} isPrivate={isPrivate} showSite={showSite} />
      ))}
    </ul>
  );
}

function SiteRow({ site, isPrivate, showSite }: { site: HistorySite } & Omit<SiteListProps, "sites">) {
  // A local file has no host, so no site pattern can name it, and it has no menu.
  const hasHost = site.host !== "";
  const label = site.host || t("history.site.noHost");
  const actions = t("history.site.actions").replace("{site}", () => site.host);

  // Asks which rule decides the site, and how much of it is recorded in all,
  // as the menu opens, so it says both as they are now.
  const showMenu = async (at: { x: number; y: number }) => {
    const url = `https://${site.host}/`;
    const loaded = await forLatestMenu(() =>
      Promise.all([isPrivate ? null : siteRuleFor(url, site.host), countVisits(siteFilter(site.host))]),
    );
    if (loaded === null) {
      return;
    }
    const [siteRule, siteVisits] = loaded;
    openMenu({ at, label: site.host, items: siteItems(site.host, url, { isPrivate, siteRule, siteVisits }) });
  };

  return (
    // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Right-click opens the row's menu; its actions button opens the same from the keyboard.
    <li
      className="group hover:bg-text/6 flex min-h-9 items-center gap-1 rounded-(--radius-control) pr-1 transition-colors duration-100"
      onContextMenu={(event) => {
        if (!hasHost) {
          return;
        }
        event.preventDefault();
        void showMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      <button
        type="button"
        className="flex min-w-0 flex-1 items-center gap-3 self-stretch px-2 text-left"
        disabled={!hasHost}
        onClick={() => showSite(site.host)}
      >
        <Favicon key={site.host} url={isPrivate || !hasHost ? null : `https://${site.host}/`} />
        <span className="min-w-0 flex-1 truncate">{label}</span>
        <span className="text-text-secondary shrink-0 text-sm">
          {counted(site.visits, "history.row.visits.one", "history.row.visits.many")}
        </span>
        <time
          className="text-text-secondary shrink-0 text-sm tabular-nums"
          dateTime={new Date(site.lastVisitedAt).toISOString()}
        >
          {new Date(site.lastVisitedAt).toLocaleDateString()}
        </time>
      </button>
      {hasHost ? (
        <button
          type="button"
          className="btn icon ghost p-sm opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"
          aria-label={actions}
          title={actions}
          aria-haspopup="menu"
          onClick={(event) => {
            const box = event.currentTarget.getBoundingClientRect();
            void showMenu({ x: box.left, y: box.bottom });
          }}
        >
          <i className="ic-ellipsis ic-sm" aria-hidden="true" />
        </button>
      ) : (
        // Keeps the columns of a row with no menu level with the others.
        <span className="btn icon ghost p-sm invisible" aria-hidden="true">
          <i className="ic-ellipsis ic-sm" />
        </span>
      )}
    </li>
  );
}
