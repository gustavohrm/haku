import { describe, expect, it } from "vitest";

import { dialogTitle, toAnswer } from "./page-dialog";

describe("dialogTitle", () => {
  it("names the site that opened the dialog", () => {
    expect(dialogTitle("https://www.tabnews.com.br/some/post")).toBe("www.tabnews.com.br says");
  });

  it("falls back to a generic title when the page has no host", () => {
    expect(dialogTitle("about:blank")).toBe("This page says");
    expect(dialogTitle("not a url")).toBe("This page says");
  });
});

describe("toAnswer", () => {
  it("accepts an alert however it was closed", () => {
    expect(toAnswer("alert", undefined)).toEqual({ action: "accept", text: null });
  });

  it("passes a prompt's text through, and dismisses a cancelled prompt", () => {
    expect(toAnswer("prompt", "typed")).toEqual({ action: "accept", text: "typed" });
    expect(toAnswer("prompt", "")).toEqual({ action: "accept", text: "" });
    expect(toAnswer("prompt", null)).toEqual({ action: "dismiss" });
  });

  it("accepts a confirmation or a leave prompt only on an explicit yes", () => {
    expect(toAnswer("confirm", true)).toEqual({ action: "accept", text: null });
    expect(toAnswer("confirm", false)).toEqual({ action: "dismiss" });
    expect(toAnswer("beforeUnload", true)).toEqual({ action: "accept", text: null });
  });
});
