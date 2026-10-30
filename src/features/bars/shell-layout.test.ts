import { describe, expect, it } from "vitest";

import { shellLayout } from "./shell-layout";

describe("shellLayout", () => {
  it("gives horizontal tabs and docked bars before settings load", () => {
    expect(shellLayout(null, "normal")).toEqual({ tabs: "horizontal", bars: "docked" });
  });

  it("follows the settings in a browser window", () => {
    expect(shellLayout({ tabLayout: "vertical", barLayout: "floating" }, "normal")).toEqual({
      tabs: "vertical",
      bars: "floating",
    });
  });

  it("keeps a popup's bar docked whatever the settings say", () => {
    expect(shellLayout({ tabLayout: "vertical", barLayout: "floating" }, "popup")).toEqual({
      tabs: "horizontal",
      bars: "docked",
    });
  });
});
