import { type HistoryVisit } from "@bindings";

import { startOfDay } from "./page-filter";

/** Every visit to one page in one day, newest first. */
export interface VisitRow {
  /** Unique within the list: the day and the URL. */
  key: string;
  url: string;
  host: string;
  /** The newest visit's title, which is the page's latest. */
  title: string;
  visits: HistoryVisit[];
}

export interface DayGroup {
  /** Local midnight of the day. */
  day: number;
  rows: VisitRow[];
}

/**
 * Groups visits, newest first, under their local day, with repeat visits to a
 * page in a day folded into one row placed at the newest of them.
 *
 * Done here rather than in Rust because a day is local and its length changes
 * with daylight saving. Regrouping everything loaded so far, rather than each
 * page as it arrives, merges a day or a row that a page boundary split.
 *
 * @param visits - Newest first, as `query_history` returns them.
 */
export function groupByDay(visits: readonly HistoryVisit[]): DayGroup[] {
  const groups: DayGroup[] = [];
  const rows = new Map<string, VisitRow>();
  for (const visit of visits) {
    const day = startOfDay(visit.visitedAt);
    let group = groups.at(-1);
    if (group?.day !== day) {
      group = { day, rows: [] };
      groups.push(group);
    }
    const key = `${day} ${visit.url}`;
    const row = rows.get(key);
    if (row) {
      row.visits.push(visit);
    } else {
      const created = { key, url: visit.url, host: visit.host, title: visit.title, visits: [visit] };
      rows.set(key, created);
      group.rows.push(created);
    }
  }
  return groups;
}
