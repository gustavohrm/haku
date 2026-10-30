import { describe, expect, it } from "vitest";

import { DEFAULT_SECTION, searchSettings, SECTIONS, sectionOf, sectionUrl } from "./sections";

describe("sectionOf", () => {
  it("opens the default section at the settings page itself", () => {
    expect(sectionOf("haku://settings")).toBe(DEFAULT_SECTION);
    expect(sectionOf("haku://settings/")).toBe(DEFAULT_SECTION);
  });

  it("reads the section from the first path segment", () => {
    expect(sectionOf("haku://settings/tabs").id).toBe("tabs");
    expect(sectionOf("haku://settings/extensions/more?x#y").id).toBe("extensions");
  });

  it("falls back to the default section for one that does not exist", () => {
    expect(sectionOf("haku://settings/nope")).toBe(DEFAULT_SECTION);
  });

  it("round-trips every section through its address", () => {
    for (const section of SECTIONS) {
      expect(sectionOf(sectionUrl(section))).toBe(section);
    }
  });
});

describe("searchSettings", () => {
  const ids = (query: string) => searchSettings(query).flatMap((result) => result.rows.map((row) => row.id));

  it("finds nothing for an empty query", () => {
    expect(searchSettings("   ")).toEqual([]);
  });

  it("matches titles regardless of case", () => {
    expect(ids("THEME")).toEqual(["theme"]);
  });

  it("matches keywords, not only what the row shows", () => {
    expect(ids("dark mode")).toContain("theme");
    expect(ids("sidebar")).toContain("tabLayout");
    expect(ids("auto-hide")).toContain("barLayout");
    expect(ids("duckduckgo")).toContain("searchUrl");
  });

  it("needs every word to match, in any order", () => {
    expect(ids("memory background")).toContain("keptMemory");
    expect(ids("background zebra")).toEqual([]);
  });

  it("groups matches under their section, in page order", () => {
    // The Tabs layout is in Appearance, first; "Loaded tabs" and the
    // background-tab rows are in Tabs & memory; the home page, "Where new tabs
    // and windows open", is in Browsing, which comes after.
    expect(searchSettings("tabs").map((result) => result.section.id)).toEqual(["appearance", "tabs", "browsing"]);
    expect(ids("tabs")).toEqual(["tabLayout", "capacity", "keptMemory", "freeze", "discard", "keptSites", "homeUrl"]);
  });
});

describe("sections", () => {
  it("give every row an id unique across the page", () => {
    const rows = SECTIONS.flatMap((section) => section.groups.flatMap((group) => group.rows.map((row) => row.id)));
    expect(new Set(rows).size).toBe(rows.length);
  });
});
