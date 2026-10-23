import { describe, expect, it } from "vitest";

import { describeCrash } from "./crash-style";

describe("describeCrash", () => {
  it("keeps a Chromium stack, which already opens with the message", () => {
    const error = new Error("bad row");
    error.stack = "Error: bad row\n    at HistoryPage (history-page.tsx:84:12)";
    expect(describeCrash(error)).toBe(error.stack);
  });

  it("puts the message in front of a WebKit stack, which has only frames", () => {
    const error = new Error("bad row");
    error.stack = "HistoryPage@history-page.tsx:84:12";
    expect(describeCrash(error)).toBe("Error: bad row\nHistoryPage@history-page.tsx:84:12");
  });

  it("describes something thrown that is not an error", () => {
    expect(describeCrash("plain")).toBe("plain");
  });
});
