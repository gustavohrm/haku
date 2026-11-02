import { describe, expect, it } from "vitest";

import { bookmarksPlace, shellLayout } from "./shell-layout";

const vertical = {
  tabLayout: "vertical",
  barLayout: "floating",
  sidebarSide: "right",
  toolbarPlacement: "sidebar",
  sidebarWidth: 300,
} as const;

describe("shellLayout", () => {
  it("gives horizontal tabs and docked bars before settings load", () => {
    expect(shellLayout(null, "normal", false)).toEqual({
      tabs: "horizontal",
      bars: "docked",
      side: "left",
      toolbar: "top",
      sidebarWidth: 240,
    });
  });

  it("follows the settings in a browser window", () => {
    expect(shellLayout(vertical, "normal", false)).toEqual({
      tabs: "vertical",
      bars: "floating",
      side: "right",
      toolbar: "sidebar",
      sidebarWidth: 300,
    });
  });

  it("keeps the toolbar on top when the tabs are not in a column", () => {
    expect(shellLayout({ ...vertical, tabLayout: "horizontal" }, "normal", false)).toMatchObject({
      tabs: "horizontal",
      toolbar: "top",
    });
  });

  it("keeps a popup's bar docked whatever the settings say", () => {
    expect(shellLayout(vertical, "popup", false)).toMatchObject({
      tabs: "horizontal",
      bars: "docked",
      toolbar: "top",
    });
  });

  it("keeps a private window's bars docked but follows its tab layout", () => {
    expect(shellLayout(vertical, "normal", true)).toMatchObject({
      tabs: "vertical",
      bars: "docked",
      side: "right",
      toolbar: "sidebar",
    });
  });

  it("keeps a private popup's bar docked with no tabs", () => {
    expect(shellLayout(vertical, "popup", true)).toMatchObject({
      tabs: "horizontal",
      bars: "docked",
    });
  });
});

describe("bookmarksPlace", () => {
  const top = { toolbar: "top" } as const;
  const column = { toolbar: "sidebar" } as const;
  const shown = (bookmarksBar: "always" | "newTab" | "never", sidebarBookmarks: "row" | "button" = "button") => ({
    bookmarksBar,
    sidebarBookmarks,
  });

  it("shows the bar on the new-tab page by default, and nowhere else", () => {
    expect(bookmarksPlace(shown("newTab"), top, "normal", true)).toBe("bar");
    expect(bookmarksPlace(shown("newTab"), top, "normal", false)).toBeNull();
  });

  it("shows it always, or never, as set", () => {
    expect(bookmarksPlace(shown("always"), top, "normal", false)).toBe("bar");
    expect(bookmarksPlace(shown("never"), top, "normal", true)).toBeNull();
  });

  it("is a button in the column, whatever the bar is set to, unless asked for a row", () => {
    expect(bookmarksPlace(shown("never"), column, "normal", false)).toBe("button");
    expect(bookmarksPlace(shown("always", "row"), column, "normal", false)).toBe("row");
    expect(bookmarksPlace(shown("newTab", "row"), column, "normal", false)).toBeNull();
  });

  it("shows nothing before settings load or in a popup", () => {
    expect(bookmarksPlace(null, top, "normal", true)).toBeNull();
    expect(bookmarksPlace(shown("always"), top, "popup", false)).toBeNull();
  });
});
