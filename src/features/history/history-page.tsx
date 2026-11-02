import { InternalPage } from "@app/internal-page";
import { type InternalPageProps } from "@app/router";
import { openMenu } from "@features/context-menu/store";
import { MENU_GAP } from "@features/overlays/use-menu-dismiss";
import { useBrowserState } from "@ipc/hooks";
import { t } from "@shared/i18n";
import { useCallback, useEffect, useMemo, useState } from "react";

import { groupByDay } from "./group";
import { clearMenu, confirmDelete, counted, deleteVisits } from "./history-actions";
import { HistoryFilters } from "./history-filters";
import { filterFromUrl, isFiltered, toHistoryFilter, withPattern, type PageFilter } from "./page-filter";
import { RecordingBanner, RecordingControls } from "./recording";
import { SiteList } from "./site-list";
import { useSites, useVisits } from "./use-history";
import { VisitList } from "./visit-list";

/** How long typing rests before the text is searched. */
export const SEARCH_DELAY_MS = 200;

type View = "visits" | "sites";

/** The filter, with its text applied once typing rests and the rest at once. */
function useSettled(filter: PageFilter): PageFilter {
  const [text, setText] = useState(filter.text);
  useEffect(() => {
    const timer = setTimeout(() => setText(filter.text), SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
  }, [filter.text]);
  return useMemo(() => ({ ...filter, text }), [filter, text]);
}

/**
 * Every recorded visit, filtered, grouped by day or by site, and deleted by
 * the row, the selection, the filter or a range of time.
 * [History § The history page](../../../docs/specs/history.md#the-history-page).
 */
export function HistoryPage({ tabId, url }: InternalPageProps) {
  const { private: isPrivate } = useBrowserState();
  // Read once: the page never writes its filter back to the address.
  const [filter, setFilter] = useState<PageFilter>(() => filterFromUrl(url));
  const [view, setView] = useState<View>("visits");
  // Read when the filter changes, not on every render, so "today" does not
  // become a new query each time the page draws.
  const [now, setNow] = useState(Date.now);
  const [selected, setSelected] = useState<ReadonlySet<number>>(new Set());

  const settled = useSettled(filter);
  const query = useMemo(() => toHistoryFilter(settled, now), [settled, now]);
  const visits = useVisits(query);
  const sites = useSites(query);
  const groups = useMemo(() => groupByDay(visits.items), [visits.items]);

  const changeFilter = (next: PageFilter) => {
    setFilter(next);
    setNow(Date.now());
    setSelected(new Set());
  };
  const showSite = useCallback((host: string) => {
    setFilter((current) => withPattern(current, "include", { kind: "site", value: host }));
    setNow(Date.now());
    setSelected(new Set());
    setView("visits");
  }, []);
  const select = (ids: number[], on: boolean) =>
    setSelected((current) => {
      const next = new Set(current);
      for (const id of ids) {
        if (on) {
          next.add(id);
        } else {
          next.delete(id);
        }
      }
      return next;
    });

  const invalid = view === "visits" ? visits.invalid : sites.invalid;
  const empty = view === "visits" ? visits.items.length === 0 : sites.items.length === 0;
  const loading = view === "visits" ? visits.loading : sites.loading;

  return (
    <InternalPage
      title={t("history.title")}
      actions={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <RecordingControls />
          <button
            type="button"
            className="btn destructive ghost edged"
            aria-haspopup="menu"
            onClick={(event) => {
              const box = event.currentTarget.getBoundingClientRect();
              openMenu({
                at: { x: box.left, y: box.bottom + MENU_GAP },
                label: t("history.clear.title"),
                items: clearMenu(),
              });
            }}
          >
            {t("history.clear")}
          </button>
        </div>
      }
    >
      <RecordingBanner />
      <HistoryFilters filter={filter} onChange={changeFilter} />
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex gap-1">
          {(["visits", "sites"] as const).map((option) => (
            <button
              key={option}
              type="button"
              aria-pressed={view === option}
              className={view === option ? "btn soft" : "btn ghost"}
              onClick={() => setView(option)}
            >
              {t(option === "visits" ? "history.view.visits" : "history.view.sites")}
            </button>
          ))}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {selected.size > 0 && (
            <button
              type="button"
              className="btn destructive soft"
              onClick={() => {
                const ids = [...selected];
                setSelected(new Set());
                void deleteVisits(ids);
              }}
            >
              {counted(selected.size, "history.deleteSelected.one", "history.deleteSelected.many")}
            </button>
          )}
          {isFiltered(settled) && (
            <button
              type="button"
              className="btn destructive ghost"
              onClick={() => void confirmDelete(query, t("history.deleteMatching.title"))}
            >
              {t("history.deleteMatching")}
            </button>
          )}
        </div>
      </div>
      {invalid !== null ? (
        <p role="alert" className="text-destructive">
          {invalid}
        </p>
      ) : empty ? (
        loading ? null : (
          <p className="text-text-secondary">
            {isFiltered(settled) ? t("history.empty.filtered") : t("history.empty")}
          </p>
        )
      ) : view === "visits" ? (
        <VisitList
          tabId={tabId}
          groups={groups}
          now={now}
          isPrivate={isPrivate}
          selected={selected}
          onSelect={select}
          showSite={showSite}
          more={visits.more}
          loading={visits.loading}
          loadMore={visits.loadMore}
        />
      ) : (
        <SiteList sites={sites.items} isPrivate={isPrivate} showSite={showSite} />
      )}
    </InternalPage>
  );
}
