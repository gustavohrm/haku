import { describe, expect, it } from "vitest";

import { dialogTitle, isWaiting, toAnswer } from "./page-dialog";

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

describe("isWaiting", () => {
  it("counts a permission request like a page dialog", () => {
    const alert = { id: 1, kind: "alert", message: "Hi", defaultText: "", url: "https://a.test" } as const;
    const camera = { id: 1, kind: "camera", origin: "https://a.test" } as const;
    expect(isWaiting({ dialog: null, permission: null })).toBe(false);
    expect(isWaiting({ dialog: alert, permission: null })).toBe(true);
    expect(isWaiting({ dialog: null, permission: camera })).toBe(true);
  });
});
