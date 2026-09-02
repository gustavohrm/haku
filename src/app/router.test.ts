import { describe, expect, it } from "vitest";

import { isInternalUrl, renderInternal } from "./router";

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

  it("renders each known route", () => {
    for (const url of ["haku:new-tab", "haku:settings", "haku:history"]) {
      expect(renderInternal(url), url).not.toBeNull();
    }
  });

  it("returns nothing for an unknown route", () => {
    expect(renderInternal("haku:nope")).toBeNull();
  });
});
