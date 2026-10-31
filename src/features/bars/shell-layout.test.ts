import { describe, expect, it } from "vitest";

import { shellLayout } from "./shell-layout";

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
