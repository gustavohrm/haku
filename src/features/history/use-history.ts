import { type HistoryCursor, type HistoryFilter, type HistorySite, type HistoryVisit } from "@bindings";
import { commands } from "@ipc/commands";
import { useCallback, useEffect, useRef, useState } from "react";

import { onHistoryChanged } from "./history-actions";

/** Visits loaded per request. */
export const PAGE_SIZE = 100;
/** Sites listed at most. */
export const SITES_LIMIT = 500;

export interface Loaded<T> {
  items: T[];
  /** The filter could not be used; the reason is shown in its place. */
  invalid: string | null;
  loading: boolean;
}

/** Counts the times this window deleted history, so a list showing it reloads. */
function useHistoryChanges(): number {
  const [changes, setChanges] = useState(0);
  useEffect(() => onHistoryChanged(() => setChanges((count) => count + 1)), []);
  return changes;
}

/** The filter and the deletions a response was asked for under, so a stale one shows as loading. */
const requestOf = (filter: HistoryFilter, changes: number) => `${changes} ${JSON.stringify(filter)}`;
const filterIn = (request: string) => JSON.parse(request.slice(request.indexOf(" ") + 1)) as HistoryFilter;

interface VisitsState {
  request: string;
  items: HistoryVisit[];
  next: HistoryCursor | null;
  invalid: string | null;
}

/**
 * One page of visits, as the change it makes to what is loaded.
 *
 * @param request - The filter and deletions it is asked under.
 * @param after - Where the page starts; `null` for the first.
 * @returns The update, or `null` when the query failed for a reason the command wrapper reports.
 */
async function fetchPage(
  request: string,
  after: HistoryCursor | null,
): Promise<((current: VisitsState) => VisitsState) | null> {
  const result = await commands.queryHistory(filterIn(request), after, PAGE_SIZE);
  if (result.status === "error") {
    if (result.error.kind !== "InvalidPattern") {
      return null;
    }
    const invalid = result.error.message;
    return () => ({ request, items: [], next: null, invalid });
  }
  const { visits, next } = result.data;
  return (current) => ({
    request,
    items: after === null ? visits : [...current.items, ...visits],
    next,
    invalid: null,
  });
}

/**
 * The visits a filter covers, newest first, a page at a time.
 *
 * A response that arrives after the filter changed is dropped, so a slow
 * query never replaces a newer one's rows. Until the new one arrives, the
 * old rows stay, so the list does not flash empty while typing.
 */
export function useVisits(filter: HistoryFilter): Loaded<HistoryVisit> & { more: boolean; loadMore: () => void } {
  const changes = useHistoryChanges();
  const request = requestOf(filter, changes);
  const latest = useRef(request);
  const [state, setState] = useState<VisitsState>({ request: "", items: [], next: null, invalid: null });
  const [loadingMore, setLoadingMore] = useState(false);

  useEffect(() => {
    latest.current = request;
    void (async () => {
      const update = await fetchPage(request, null);
      if (update !== null && request === latest.current) {
        setState(update);
      }
    })();
  }, [request]);

  const current = state.request === request;
  const loadMore = useCallback(async () => {
    if (!current || loadingMore || state.next === null) {
      return;
    }
    setLoadingMore(true);
    try {
      const update = await fetchPage(request, state.next);
      if (update !== null && request === latest.current) {
        setState(update);
      }
    } finally {
      setLoadingMore(false);
    }
  }, [current, loadingMore, state.next, request]);

  return {
    items: state.items,
    invalid: state.invalid,
    loading: !current || loadingMore,
    more: current && state.next !== null,
    loadMore: () => void loadMore(),
  };
}

/** The sites a filter covers, most visited first. */
export function useSites(filter: HistoryFilter): Loaded<HistorySite> {
  const changes = useHistoryChanges();
  const request = requestOf(filter, changes);
  const [state, setState] = useState<Omit<Loaded<HistorySite>, "loading"> & { request: string }>({
    request: "",
    items: [],
    invalid: null,
  });

  useEffect(() => {
    let current = true;
    void (async () => {
      const result = await commands.historySites(filterIn(request), SITES_LIMIT);
      if (!current) {
        return;
      }
      if (result.status === "ok") {
        setState({ request, items: result.data, invalid: null });
      } else {
        const invalid = result.error.kind === "InvalidPattern" ? result.error.message : null;
        setState({ request, items: [], invalid });
      }
    })();
    return () => {
      current = false;
    };
  }, [request]);

  return { items: state.items, invalid: state.invalid, loading: state.request !== request };
}
