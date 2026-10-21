import type { HakuError } from "@bindings";
import { describe, expect, it } from "vitest";

import { describeError, HakuFailure, unwrap } from "./result";

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

describe("describeError", () => {
  it("describes a missing webview slot in English", () => {
    expect(describeError({ kind: "NoSlotAvailable" })).toBe("No webview slot is available.");
  });

  it("describes a missing tab with its identifier", () => {
    expect(describeError({ kind: "TabNotFound", message: "7" })).toBe("Tab not found: 7");
  });

  it("describes an invalid address with its message", () => {
    expect(describeError({ kind: "InvalidUrl", message: "nope" })).toBe("Invalid URL: nope");
  });

  it("describes an unsupported operation with its message", () => {
    expect(describeError({ kind: "Unsupported", message: "print" })).toBe("Unsupported: print");
  });

  it("describes a missing window or webview with its message", () => {
    expect(describeError({ kind: "WindowMissing", message: "main" })).toBe("Window or webview missing: main");
  });

  it("describes a storage failure with its message", () => {
    expect(describeError({ kind: "Storage", message: "disk full" })).toBe("Storage failure: disk full");
  });

  it("describes a Tauri failure with its message", () => {
    expect(describeError({ kind: "Tauri", message: "ipc" })).toBe("Tauri failure: ipc");
  });
});
