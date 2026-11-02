import { type HistoryFilter, type HistoryRetention } from "@bindings";

/** The filter that covers every visit. */
export const EVERYTHING: HistoryFilter = {
  text: null,
  from: null,
  to: null,
  include: [],
  exclude: [],
  transitions: [],
};

/**
 * A filter covering every visit, narrowed by `patch`.
 *
 * @param patch - The fields that narrow it.
 * @returns The filter.
 */
export function filterOf(patch: Partial<HistoryFilter>): HistoryFilter {
  return { ...EVERYTHING, ...patch };
}

const DAY_MS = 24 * 60 * 60 * 1000;

const RETENTION_DAYS: Record<HistoryRetention, number | null> = {
  forever: null,
  days30: 30,
  days90: 90,
  year: 365,
};

/**
 * The time before which visits are deleted, as Rust's `HistoryRetention::cutoff` works it out.
 *
 * @param retention - How long visits are kept.
 * @param now - Milliseconds since the epoch.
 * @returns The cutoff in milliseconds, or `null` when visits are kept forever.
 */
export function retentionCutoff(retention: HistoryRetention, now: number): number | null {
  const days = RETENTION_DAYS[retention];
  return days === null ? null : now - days * DAY_MS;
}

/**
 * Whether keeping visits for `next` instead of `current` deletes some.
 *
 * @param current - How long visits are kept now.
 * @param next - How long they would be kept.
 * @returns True when `next` keeps them for less time.
 */
export function shortens(current: HistoryRetention, next: HistoryRetention): boolean {
  const keeps = (retention: HistoryRetention) => RETENTION_DAYS[retention] ?? Number.POSITIVE_INFINITY;
  return keeps(next) < keeps(current);
}
