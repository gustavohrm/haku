import { describe, expect, it } from "vitest";

import {
  addDays,
  chipsOf,
  filterFromUrl,
  isFiltered,
  NO_FILTER,
  startOfDay,
  toHistoryFilter,
  withPattern,
  type PageFilter,
} from "./page-filter";

// A local time in the middle of a day, so every case is about whole days.
const NOW = new Date(2026, 10, 2, 15, 30).getTime();
const day = (month: number, date: number) => new Date(2026, month, date).getTime();
const site = (value: string) => ({ kind: "site" as const, value });

describe("toHistoryFilter", () => {
  it("covers everything when nothing is chosen", () => {
    expect(toHistoryFilter(NO_FILTER, NOW)).toEqual({
      text: null,
      from: null,
      to: null,
      include: [],
      exclude: [],
      transitions: [],
    });
  });

  it("trims the text and drops it when blank", () => {
    expect(toHistoryFilter({ ...NO_FILTER, text: "  rust  " }, NOW).text).toBe("rust");
    expect(toHistoryFilter({ ...NO_FILTER, text: "   " }, NOW).text).toBeNull();
  });

  it.each([
    ["today", day(10, 2), null],
    ["yesterday", day(10, 1), day(10, 2)],
    ["days7", day(9, 27), null],
    ["days30", day(9, 4), null],
  ] as const)("reads %s as whole local days", (when, from, to) => {
    expect(toHistoryFilter({ ...NO_FILTER, when }, NOW)).toMatchObject({ from, to });
  });

  it("includes the whole last day of a custom range", () => {
    const filter: PageFilter = { ...NO_FILTER, when: "custom", fromDay: "2026-10-01", toDay: "2026-10-03" };
    expect(toHistoryFilter(filter, NOW)).toMatchObject({ from: day(9, 1), to: day(9, 4) });
  });

  it("leaves an empty or malformed end of a custom range open", () => {
    const filter: PageFilter = { ...NO_FILTER, when: "custom", fromDay: "", toDay: "nonsense" };
    expect(toHistoryFilter(filter, NOW)).toMatchObject({ from: null, to: null });
  });
});

describe("days", () => {
  it("starts a day at local midnight", () => {
    expect(startOfDay(NOW)).toBe(day(10, 2));
  });

  it("moves by calendar days", () => {
    expect(addDays(day(10, 2), -2)).toBe(day(9, 31));
  });
});

describe("isFiltered", () => {
  it("is false only for a filter that narrows nothing", () => {
    expect(isFiltered(NO_FILTER)).toBe(false);
    expect(isFiltered({ ...NO_FILTER, text: " " })).toBe(false);
    expect(isFiltered({ ...NO_FILTER, text: "a" })).toBe(true);
    expect(isFiltered({ ...NO_FILTER, when: "today" })).toBe(true);
    expect(isFiltered({ ...NO_FILTER, exclude: [site("a.com")] })).toBe(true);
    expect(isFiltered({ ...NO_FILTER, transitions: ["typed"] })).toBe(true);
  });

  it("is false for a custom range with neither end set, which would cover every visit", () => {
    expect(isFiltered({ ...NO_FILTER, when: "custom" })).toBe(false);
    expect(isFiltered({ ...NO_FILTER, when: "custom", fromDay: "nonsense" })).toBe(false);
    expect(isFiltered({ ...NO_FILTER, when: "custom", toDay: "2026-01-02" })).toBe(true);
  });
});

describe("chipsOf", () => {
  const describe = { when: () => "When", pattern: (p: { value: string }) => p.value, transition: (t: string) => t };

  it("gives one chip per narrowing besides the text, each removing only itself", () => {
    const filter: PageFilter = {
      ...NO_FILTER,
      text: "ignored",
      when: "custom",
      fromDay: "2026-10-01",
      include: [site("a.com"), site("b.com")],
      exclude: [site("c.com")],
      transitions: ["typed", "link"],
    };
    const chips = chipsOf(filter, describe);
    expect(chips.map((chip) => [chip.kind, chip.label])).toEqual([
      ["when", "When"],
      ["include", "a.com"],
      ["include", "b.com"],
      ["exclude", "c.com"],
      ["transition", "typed"],
      ["transition", "link"],
    ]);
    expect(chips[0]!.remove(filter)).toMatchObject({ when: "any", fromDay: "", toDay: "" });
    expect(chips[1]!.remove(filter).include).toEqual([site("b.com")]);
    expect(chips[3]!.remove(filter).exclude).toEqual([]);
    expect(chips[4]!.remove(filter).transitions).toEqual(["link"]);
  });

  it("gives none for no filter", () => {
    expect(chipsOf(NO_FILTER, describe)).toEqual([]);
  });
});

describe("withPattern", () => {
  it("adds a pattern once and takes it out of the other list", () => {
    const excluded = withPattern(NO_FILTER, "exclude", site("a.com"));
    const included = withPattern(withPattern(excluded, "include", site("a.com")), "include", site("a.com"));
    expect(included.include).toEqual([site("a.com")]);
    expect(included.exclude).toEqual([]);
  });
});

describe("filterFromUrl", () => {
  it("reads the text and the site", () => {
    expect(filterFromUrl("haku://history?q=rust%20book&site=docs.rs")).toEqual({
      ...NO_FILTER,
      text: "rust book",
      include: [site("docs.rs")],
    });
  });

  it("ignores a blank site and an address with neither", () => {
    expect(filterFromUrl("haku://history?site=%20")).toEqual(NO_FILTER);
    expect(filterFromUrl("haku://history")).toEqual(NO_FILTER);
  });
});
