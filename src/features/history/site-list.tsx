import { type HistorySite } from "@bindings";
import { Favicon } from "@features/bookmarks/favicon";
import { t } from "@shared/i18n";

import { counted } from "./history-actions";

interface SiteListProps {
  sites: HistorySite[];
  isPrivate: boolean;
  /** Opens the visits view narrowed to the site. */
  showSite: (host: string) => void;
}

/** One row per site with its visit count and last visit, most visited first. */
export function SiteList({ sites, isPrivate, showSite }: SiteListProps) {
  return (
    <ul className="flex flex-col">
      {sites.map((site) => (
        <li key={site.host}>
          <button
            type="button"
            className="hover:bg-text/6 flex w-full items-center gap-3 rounded-(--radius-control) px-3 py-2 text-left"
            // A local file has no host, so no site pattern can name it.
            disabled={site.host === ""}
            onClick={() => showSite(site.host)}
          >
            <Favicon key={site.host} url={isPrivate || site.host === "" ? null : `https://${site.host}/`} />
            <span className="min-w-0 flex-1 truncate font-medium">{site.host || t("history.site.noHost")}</span>
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
        </li>
      ))}
    </ul>
  );
}
