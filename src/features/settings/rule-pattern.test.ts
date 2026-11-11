import { describe, expect, it } from "vitest";

import { guessPattern, moveItem } from "./rule-pattern";

describe("guessPattern", () => {
  it("reads a host as a site", () => {
    expect(guessPattern("mail.example.com")).toEqual({ kind: "site", value: "mail.example.com" });
  });

  it("reads an address with nothing after its host as a site", () => {
    expect(guessPattern("https://example.com/")).toEqual({ kind: "site", value: "https://example.com/" });
  });

  it("reads an address with a path or query as a page", () => {
    expect(guessPattern("https://example.com/a?b=1")).toEqual({ kind: "page", value: "https://example.com/a?b=1" });
    expect(guessPattern("https://example.com/?q=1")).toEqual({ kind: "page", value: "https://example.com/?q=1" });
  });

  it("reads anything with a star as a wildcard", () => {
    expect(guessPattern("*://*.bank.com/*")).toEqual({ kind: "wildcard", value: "*://*.bank.com/*" });
    expect(guessPattern("*token=*")).toEqual({ kind: "wildcard", value: "*token=*" });
  });

  it("reads a pattern between slashes as a regular expression, without the slashes", () => {
    expect(guessPattern("/^https://[^/]+/admin/")).toEqual({ kind: "regex", value: "^https://[^/]+/admin" });
  });

  it("trims what was typed", () => {
    expect(guessPattern("  example.com ")).toEqual({ kind: "site", value: "example.com" });
  });
});

describe("moveItem", () => {
  it("moves an item down, to the place it was dropped", () => {
    expect(moveItem(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"]);
  });

  it("moves an item up", () => {
    expect(moveItem(["a", "b", "c", "d"], 3, 1)).toEqual(["a", "d", "b", "c"]);
  });

  it("leaves the list as it was for a move out of it", () => {
    expect(moveItem(["a", "b"], 0, 5)).toEqual(["a", "b"]);
    expect(moveItem(["a", "b"], 1, -1)).toEqual(["a", "b"]);
  });
});
