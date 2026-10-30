import { describe, expect, it } from "vitest";

import { shellLayout } from "./shell-layout";

describe("shellLayout", () => {
  it("gives horizontal tabs and docked bars before settings load", () => {
    expect(shellLayout(null, "normal", false)).toEqual({ tabs: "horizontal", bars: "docked" });
  });

  it("follows the settings in a browser window", () => {
    expect(shellLayout({ tabLayout: "vertical", barLayout: "floating" }, "normal", false)).toEqual({
      tabs: "vertical",
      bars: "floating",
    });
  });

  it("keeps a popup's bar docked whatever the settings say", () => {
    expect(shellLayout({ tabLayout: "vertical", barLayout: "floating" }, "popup", false)).toEqual({
      tabs: "horizontal",
      bars: "docked",
    });
  });

  it("keeps a private window's bars docked but follows its tab layout", () => {
    expect(shellLayout({ tabLayout: "vertical", barLayout: "floating" }, "normal", true)).toEqual({
      tabs: "vertical",
      bars: "docked",
    });
  });

  it("keeps a private popup's bar docked with no tabs", () => {
    expect(shellLayout({ tabLayout: "vertical", barLayout: "floating" }, "popup", true)).toEqual({
      tabs: "horizontal",
      bars: "docked",
    });
  });
});
