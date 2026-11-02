import { type HistoryFilter, type Transition, type UrlPattern, type UrlPatternKind } from "@bindings";
import { t, type TranslationKey } from "@shared/i18n";

import { EVERYTHING } from "./filter";

/** A range of days the page offers, in local time. */
export type When = "any" | "today" | "yesterday" | "days7" | "days30" | "custom";

export const WHENS: Record<When, TranslationKey> = {
  any: "history.when.any",
  today: "history.when.today",
  yesterday: "history.when.yesterday",
  days7: "history.when.days7",
  days30: "history.when.days30",
  custom: "history.when.custom",
};

export const TRANSITIONS: Record<Transition, TranslationKey> = {
  typed: "history.transition.typed",
  search: "history.transition.search",
  bookmark: "history.transition.bookmark",
  link: "history.transition.link",
  backForward: "history.transition.backForward",
  other: "history.transition.other",
};

export const PATTERN_KINDS: Record<Exclude<UrlPatternKind, "page">, TranslationKey> = {
  site: "history.pattern.site",
  wildcard: "history.pattern.wildcard",
  regex: "history.pattern.regex",
};

/** Names a pattern for a chip: a site as itself, anything else with its kind. */
export function describePattern(pattern: UrlPattern): string {
  return pattern.kind === "site" || pattern.kind === "page"
    ? pattern.value
    : `${t(PATTERN_KINDS[pattern.kind])}: ${pattern.value}`;
}

/** What the page's filter controls hold, before it becomes a {@link HistoryFilter}. */
export interface PageFilter {
  text: string;
  when: When;
  /** A custom range's first and last day, as `yyyy-mm-dd`; either may be empty. */
  fromDay: string;
  toDay: string;
  include: UrlPattern[];
  exclude: UrlPattern[];
  transitions: Transition[];
}

export const NO_FILTER: PageFilter = {
  text: "",
  when: "any",
  fromDay: "",
  toDay: "",
  include: [],
  exclude: [],
  transitions: [],
};

/**
 * The page's filter as Rust reads it: a range of whole local days, the text
 * as typed, and the patterns and transitions as chosen.
 *
 * @param filter - What the controls hold.
 * @param now - The current time, in milliseconds since the epoch.
 * @returns The filter to query with.
 */
export function toHistoryFilter(filter: PageFilter, now: number): HistoryFilter {
  const text = filter.text.trim();
  return {
    ...EVERYTHING,
    ...range(filter, now),
    text: text === "" ? null : text,
    include: filter.include,
    exclude: filter.exclude,
    transitions: filter.transitions,
  };
}

function range(filter: PageFilter, now: number): Pick<HistoryFilter, "from" | "to"> {
  const today = startOfDay(now);
  switch (filter.when) {
    case "any":
      return { from: null, to: null };
    case "today":
      return { from: today, to: null };
    case "yesterday":
      return { from: addDays(today, -1), to: today };
    case "days7":
      return { from: addDays(today, -6), to: null };
    case "days30":
      return { from: addDays(today, -29), to: null };
    case "custom": {
      const from = parseDay(filter.fromDay);
      const to = parseDay(filter.toDay);
      return { from, to: to === null ? null : addDays(to, 1) };
    }
  }
}

/** Midnight, local time, of the day `time` falls on. */
export function startOfDay(time: number): number {
  const date = new Date(time);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** `days` local days after midnight `day`, which daylight saving can make other than 24 hours each. */
export function addDays(day: number, days: number): number {
  const date = new Date(day);
  date.setDate(date.getDate() + days);
  return date.getTime();
}

/** Midnight, local time, of a `yyyy-mm-dd` day, or `null` for anything else. */
function parseDay(day: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  if (!match) {
    return null;
  }
  return new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3])).getTime();
}

/** Whether the filter narrows anything, so that deleting what it covers is not deleting everything. */
export function isFiltered(filter: PageFilter): boolean {
  return (
    filter.text.trim() !== "" ||
    filter.when !== "any" ||
    filter.include.length > 0 ||
    filter.exclude.length > 0 ||
    filter.transitions.length > 0
  );
}

/** One removable chip for each narrowing the filter has besides its text, which its own field shows. */
export interface Chip {
  id: string;
  kind: "when" | "include" | "exclude" | "transition";
  label: string;
  remove: (filter: PageFilter) => PageFilter;
}

/**
 * The chips for a filter.
 *
 * @param filter - What the controls hold.
 * @param describe - Names a range, a pattern or a transition for the chip.
 */
export function chipsOf(
  filter: PageFilter,
  describe: {
    when: (filter: PageFilter) => string;
    pattern: (pattern: UrlPattern) => string;
    transition: (transition: Transition) => string;
  },
): Chip[] {
  const chips: Chip[] = [];
  if (filter.when !== "any") {
    chips.push({
      id: "when",
      kind: "when",
      label: describe.when(filter),
      remove: (current) => ({ ...current, when: "any", fromDay: "", toDay: "" }),
    });
  }
  for (const [kind, patterns] of [
    ["include", filter.include],
    ["exclude", filter.exclude],
  ] as const) {
    for (const pattern of patterns) {
      chips.push({
        id: `${kind}:${pattern.kind}:${pattern.value}`,
        kind,
        label: describe.pattern(pattern),
        remove: (current) => ({ ...current, [kind]: current[kind].filter((other) => !samePattern(other, pattern)) }),
      });
    }
  }
  for (const transition of filter.transitions) {
    chips.push({
      id: `transition:${transition}`,
      kind: "transition",
      label: describe.transition(transition),
      remove: (current) => ({ ...current, transitions: current.transitions.filter((other) => other !== transition) }),
    });
  }
  return chips;
}

export function samePattern(a: UrlPattern, b: UrlPattern): boolean {
  return a.kind === b.kind && a.value === b.value;
}

/**
 * Adds a pattern to include or exclude, once, taking it out of the other list
 * where it was, since a visit cannot be both.
 */
export function withPattern(filter: PageFilter, kind: "include" | "exclude", pattern: UrlPattern): PageFilter {
  const other = kind === "include" ? "exclude" : "include";
  return {
    ...filter,
    [kind]: [...filter[kind].filter((each) => !samePattern(each, pattern)), pattern],
    [other]: filter[other].filter((each) => !samePattern(each, pattern)),
  };
}

/**
 * What the page's address asks for when it is opened: `?q=` fills the text and
 * `?site=` shows one site. Read once; the page never writes its filter back.
 *
 * @param url - The page's address.
 */
export function filterFromUrl(url: string): PageFilter {
  let params: URLSearchParams;
  try {
    params = new URL(url).searchParams;
  } catch {
    return NO_FILTER;
  }
  const site = params.get("site")?.trim() ?? "";
  return {
    ...NO_FILTER,
    text: params.get("q") ?? "",
    include: site === "" ? [] : [{ kind: "site", value: site }],
  };
}
