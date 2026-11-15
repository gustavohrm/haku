import { describe, expect, it } from "vitest";

import { describeAccess } from "./access";

describe("describeAccess", () => {
  it("names a few sites and counts the rest", () => {
    expect(describeAccess({ kind: "sites", hosts: ["a.test", "*.b.test"] })).toBe(
      "Read and change your data on a.test, *.b.test",
    );
    expect(describeAccess({ kind: "sites", hosts: ["a.test", "b.test", "c.test", "d.test", "e.test"] })).toBe(
      "Read and change your data on a.test, b.test, c.test and 2 more sites",
    );
  });

  it("says what every other access allows", () => {
    expect(describeAccess({ kind: "allSites" })).toBe("Read and change all your data on every site");
    expect(describeAccess({ kind: "nativeMessaging" })).toBe("Talk to programs installed on your computer");
  });
});
