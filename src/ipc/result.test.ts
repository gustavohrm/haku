import type { HakuError } from "@bindings";
import { describe, expect, it } from "vitest";

import { HakuFailure, unwrap } from "./result";

const failure = (error: HakuError) => Promise.resolve({ status: "error" as const, error });

describe("unwrap", () => {
  it("returns the value of a successful command", async () => {
    await expect(unwrap(Promise.resolve({ status: "ok" as const, data: 42 }))).resolves.toBe(42);
  });

  it("throws a HakuFailure when the command reported an error", async () => {
    await expect(unwrap(failure({ kind: "NoSlotAvailable" }))).rejects.toBeInstanceOf(HakuFailure);
  });

  it("keeps the typed variant on the thrown failure so callers can branch on it", async () => {
    const error = await unwrap(failure({ kind: "TabNotFound", message: "7" })).catch((thrown: unknown) => thrown);

    expect(error).toBeInstanceOf(HakuFailure);
    expect((error as HakuFailure).cause).toEqual({ kind: "TabNotFound", message: "7" });
  });

  it("describes a variant that carries a message", async () => {
    const error = await unwrap(failure({ kind: "InvalidUrl", message: "nope" })).catch((thrown: unknown) => thrown);

    expect((error as HakuFailure).message).toContain("nope");
  });

  it("describes a variant that carries no message", async () => {
    const error = await unwrap(failure({ kind: "NoSlotAvailable" })).catch((thrown: unknown) => thrown);

    expect((error as HakuFailure).message).toBe("No webview slot is available.");
  });
});
