import { describe, expect, it } from "vitest";

import { bookmarksPlace, shellLayout } from "./shell-layout";

const vertical = {
  tabLayout: "vertical",
  barLayout: "floating",
  sidebarSide: "right",
  toolbarPlacement: "sidebar",
  sidebarWidth: 300,
  addressBarLayout: "inline",
} as const;

describe("shellLayout", () => {
  it("gives horizontal tabs and docked bars before settings load", () => {
    expect(shellLayout(null, "normal", false)).toEqual({
      tabs: "horizontal",
      bars: "docked",
      side: "left",
      toolbar: "top",
      sidebarWidth: 240,
      address: "inline",
    });
  });

  it("follows the settings in a browser window", () => {
    expect(shellLayout(vertical, "normal", false)).toEqual({
      tabs: "vertical",
      bars: "floating",
      side: "right",
      toolbar: "sidebar",
      sidebarWidth: 300,
      address: "floating",
    });
  });

  it("floats the address field in the column whatever the setting, and on top only when asked", () => {
    expect(shellLayout(vertical, "normal", false).address).toBe("floating");
    const top = { ...vertical, toolbarPlacement: "top" } as const;
    expect(shellLayout(top, "normal", false).address).toBe("inline");
    expect(shellLayout({ ...top, addressBarLayout: "floating" }, "normal", false).address).toBe("floating");
    const horizontal = { ...vertical, tabLayout: "horizontal", addressBarLayout: "floating" } as const;
    expect(shellLayout(horizontal, "normal", false).address).toBe("floating");
    expect(shellLayout(horizontal, "popup", false).address).toBe("inline");
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
    expect(bookmarksPlace(shown("newTab"), top, "normal", true, false)).toBe("bar");
    expect(bookmarksPlace(shown("newTab"), top, "normal", false, false)).toBeNull();
  });

  it("leaves an empty bar off the new-tab page, but shows it empty when asked for always", () => {
    expect(bookmarksPlace(shown("newTab"), top, "normal", true, true)).toBeNull();
    expect(bookmarksPlace(shown("newTab", "row"), column, "normal", true, true)).toBeNull();
    expect(bookmarksPlace(shown("always"), top, "normal", true, true)).toBe("bar");
    expect(bookmarksPlace(shown("newTab"), column, "normal", true, true)).toBe("button");
  });

  it("shows it always, or never, as set", () => {
    expect(bookmarksPlace(shown("always"), top, "normal", false, false)).toBe("bar");
    expect(bookmarksPlace(shown("never"), top, "normal", true, false)).toBeNull();
  });

  it("is a button in the column, whatever the bar is set to, unless asked for a row", () => {
    expect(bookmarksPlace(shown("never"), column, "normal", false, false)).toBe("button");
    expect(bookmarksPlace(shown("always", "row"), column, "normal", false, false)).toBe("row");
    expect(bookmarksPlace(shown("newTab", "row"), column, "normal", false, false)).toBeNull();
  });

  it("shows nothing before settings load or in a popup", () => {
    expect(bookmarksPlace(null, top, "normal", true, false)).toBeNull();
    expect(bookmarksPlace(shown("always"), top, "popup", false, false)).toBeNull();
  });
});
