import { describe, expect, it } from "vitest";

import { permissionName, permissionTitle, toDecision } from "./permission-prompt";

describe("permissionTitle", () => {
  it("names the origin that asked and what it asked for", () => {
    expect(permissionTitle({ id: 1, kind: "camera", origin: "https://meet.example" })).toBe(
      "https://meet.example wants to use your camera",
    );
    expect(permissionTitle({ id: 2, kind: "multipleDownloads", origin: "http://localhost:8080" })).toBe(
      "http://localhost:8080 wants to download multiple files",
    );
  });
});

describe("permissionName", () => {
  it("has a name for every kind", () => {
    expect(permissionName("location")).toBe("Location");
    expect(permissionName("clipboardRead")).toBe("Clipboard");
  });
});

describe("toDecision", () => {
  it("allows on Allow, blocks on Block, and dismisses on Escape", () => {
    expect(toDecision(true, false)).toBe("allow");
    expect(toDecision(false, false)).toBe("block");
    expect(toDecision(false, true)).toBe("dismiss");
  });
});
