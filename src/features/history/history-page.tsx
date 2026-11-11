import { InternalPage } from "@app/internal-page";
import { type InternalPageProps } from "@app/router";
import { type HistoryFilter } from "@bindings";
import { openMenu } from "@features/context-menu/store";
import { feedback } from "@features/feedback/feedback";
import { MENU_GAP } from "@features/overlays/use-menu-dismiss";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { t } from "@shared/i18n";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { groupByDay } from "./group";
import {
  clearMenu,
  counted,
  countVisits,
  deleteConfirmation,
  deleteNow,
  deleteVisits,
  forLatestMenu,
  HISTORY_SETTINGS_URL,
} from "./history-actions";
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
  const [picked, setSelected] = useState<ReadonlySet<number>>(new Set());
  // Delete all matching, while it asks: the filter as it was counted, and the count.
  const [deleting, setDeleting] = useState<{ filter: HistoryFilter; count: number } | null>(null);
  // Counts asked for so far: one that answers after the filter changed is dropped.
  const counts = useRef(0);
  // Set when the question is answered, so the button that takes its place takes its focus.
  const refocusDelete = useRef(false);

  const settled = useSettled(filter);
  const query = useMemo(() => toHistoryFilter(settled, now), [settled, now]);
  const visits = useVisits(query);
  const sites = useSites(query);
  const groups = useMemo(() => groupByDay(visits.items), [visits.items]);
  // Only what is still listed: a selected visit deleted from its own menu is no longer selected.
  const selected = useMemo(() => {
    if (picked.size === 0) {
      return picked;
    }
    const listed = new Set(visits.items.map((visit) => visit.id));
    return new Set([...picked].filter((id) => listed.has(id)));
  }, [picked, visits.items]);

  const changeFilter = (next: PageFilter) => {
    setFilter(next);
    setNow(Date.now());
    setSelected(new Set());
    setDeleting(null);
    counts.current += 1;
  };
  const showSite = useCallback((host: string) => {
    setFilter((current) => withPattern(current, "include", { kind: "site", value: host }));
    setNow(Date.now());
    setSelected(new Set());
    setDeleting(null);
    counts.current += 1;
    setView("visits");
  }, []);
  const askToDeleteMatching = async () => {
    // The filter as it shows, not as it was last searched: text typed a moment ago counts.
    if (!isFiltered(filter)) {
      return;
    }
    const at = Date.now();
    const shown = toHistoryFilter(filter, now);
    // Ending now, so the question deletes the visits it counted and none recorded while it waits.
    const bounded = { ...shown, to: shown.to === null ? at : Math.min(shown.to, at) };
    counts.current += 1;
    const asked = counts.current;
    const count = await countVisits(bounded);
    if (asked !== counts.current) {
      return;
    }
    if (count === 0) {
      feedback().info(t("history.delete.none"));
    } else if (count !== null) {
      setDeleting({ filter: bounded, count });
    }
  };
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
            className="btn ghost"
            aria-haspopup="menu"
            onClick={async (event) => {
              const box = event.currentTarget.getBoundingClientRect();
              const items = await forLatestMenu(() => clearMenu());
              if (items === null) {
                return;
              }
              openMenu({ at: { x: box.left, y: box.bottom + MENU_GAP }, label: t("history.clear.title"), items });
            }}
          >
            {t("history.clear")}
          </button>
          <button
            type="button"
            className="btn icon ghost"
            aria-label={t("history.settings")}
            title={t("history.settings")}
            onClick={() => void commands.navigateTab(tabId, HISTORY_SETTINGS_URL, "other")}
          >
            <i className="ic-settings ic-sm" aria-hidden="true" />
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
              className="btn ghost"
              onClick={() => {
                const ids = [...selected];
                setSelected(new Set());
                void deleteVisits(ids);
              }}
            >
              {counted(selected.size, "history.deleteSelected.one", "history.deleteSelected.many")}
            </button>
          )}
          {deleting !== null ? (
            <DeleteQuestion
              count={deleting.count}
              onAnswer={(yes) => {
                refocusDelete.current = true;
                setDeleting(null);
                if (yes) {
                  void deleteNow(deleting.filter);
                }
              }}
            />
          ) : (
            isFiltered(settled) && (
              <button
                ref={(button) => {
                  if (button && refocusDelete.current) {
                    refocusDelete.current = false;
                    button.focus();
                  }
                }}
                type="button"
                className="btn ghost"
                onClick={() => void askToDeleteMatching()}
              >
                {t("history.deleteMatching")}
              </button>
            )
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

/**
 * Delete all matching, asking in place: how many visits go, then **Cancel**
 * and **Delete N visits**. Focus starts on Cancel, the safe answer.
 */
function DeleteQuestion({ count, onAnswer }: { count: number; onAnswer: (yes: boolean) => void }) {
  const { message, action } = deleteConfirmation(count);
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => cancelRef.current?.focus(), []);
  return (
    <div role="alert" className="flex flex-wrap items-center gap-2">
      <span className="text-sm">{message}</span>
      <button ref={cancelRef} type="button" className="btn ghost" onClick={() => onAnswer(false)}>
        {t("dialog.cancel")}
      </button>
      <button type="button" className="btn destructive" onClick={() => onAnswer(true)}>
        {action}
      </button>
    </div>
  );
}
