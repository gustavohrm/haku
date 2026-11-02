import { type HistoryVisit } from "@bindings";
import { describe, expect, it } from "vitest";

import { groupByDay } from "./group";

let nextId = 1;
const visit = (url: string, visitedAt: number, title = url): HistoryVisit => ({
  id: nextId++,
  url,
  host: new URL(url).host,
  title,
  visitedAt,
  transition: "link",
});
const at = (date: number, hour: number) => new Date(2026, 10, date, hour).getTime();

describe("groupByDay", () => {
  it("puts visits under their local day, newest first", () => {
    const groups = groupByDay([visit("https://a.com/", at(2, 9)), visit("https://b.com/", at(1, 23))]);
    expect(groups.map((group) => group.day)).toEqual([at(2, 0), at(1, 0)]);
    expect(groups.map((group) => group.rows.map((row) => row.url))).toEqual([["https://a.com/"], ["https://b.com/"]]);
  });

  it("folds repeat visits to a page in a day into one row at the newest, titled by it", () => {
    const newest = visit("https://a.com/", at(2, 12), "New title");
    const groups = groupByDay([
      newest,
      visit("https://b.com/", at(2, 11)),
      visit("https://a.com/", at(2, 10), "Old title"),
    ]);
    expect(groups).toHaveLength(1);
    const [first, second] = groups[0]!.rows;
    expect(first).toMatchObject({ url: "https://a.com/", title: "New title", host: "a.com" });
    expect(first!.visits).toHaveLength(2);
    expect(first!.visits[0]).toBe(newest);
    expect(second!.url).toBe("https://b.com/");
  });

  it("keeps visits to the same page on different days apart", () => {
    const groups = groupByDay([visit("https://a.com/", at(2, 9)), visit("https://a.com/", at(1, 9))]);
    expect(groups.map((group) => group.rows.length)).toEqual([1, 1]);
    expect(new Set(groups.flatMap((group) => group.rows.map((row) => row.key))).size).toBe(2);
  });

  it("merges a day and a row that a page boundary split", () => {
    const first = [visit("https://a.com/", at(2, 12)), visit("https://b.com/", at(2, 11))];
    const second = [visit("https://a.com/", at(2, 10))];
    const groups = groupByDay([...first, ...second]);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.rows.map((row) => row.visits.length)).toEqual([2, 1]);
  });

  it("gives nothing for no visits", () => {
    expect(groupByDay([])).toEqual([]);
  });
});
