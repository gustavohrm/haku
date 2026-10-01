import { describe, expect, it } from "vitest";

import { internalTitle, isInternalUrl, renderInternal } from "./router";

describe("internal routing", () => {
  it("recognises an internal url", () => {
    expect(isInternalUrl("haku:settings")).toBe(true);
  });

  it("does not mistake a web url for an internal one", () => {
    expect(isInternalUrl("https://haku.example")).toBe(false);
  });

  it("returns an element whose type is the page component, not the page's output", () => {
    // Calling a component as a plain function runs its hooks inside the
    // caller's hook sequence. Opening or leaving an internal page then changes
    // that sequence and React tears down the whole tree, which shows up as the
    // entire interface vanishing.
    const element = renderInternal("haku:settings");

    expect(element).not.toBeNull();
    expect(typeof element?.type).toBe("function");
    expect(element?.props).toEqual({});
  });

  it.each(["haku:new-tab", "haku:settings", "haku:history"])("renders %s", (url) => {
    expect(renderInternal(url)).not.toBeNull();
  });

  it("names an internal page for its tab", () => {
    expect(internalTitle("haku:settings")).toBe("Settings");
  });

  it("gives a web page no internal name, even one whose path looks like a route", () => {
    expect(internalTitle("https://a.test/settings")).toBeNull();
    expect(internalTitle("haku:unknown")).toBeNull();
    expect(internalTitle("haku:constructor")).toBeNull();
  });

  it("returns nothing for an unknown route", () => {
    expect(renderInternal("haku:nope")).toBeNull();
  });
});
