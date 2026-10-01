import { describe, expect, it } from "vitest";

import { cx } from "./class-names";

describe("cx", () => {
  it("joins the names that apply and drops the rest", () => {
    const active = false;
    expect(cx("tab", active && "active", null, undefined, "pinned")).toBe("tab pinned");
  });
});
