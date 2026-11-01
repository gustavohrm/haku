import type { PermissionKind } from "@bindings";
import { describe, expect, it } from "vitest";

import { permissionAction, permissionName, permissionTitle, toDecision } from "./permission-prompt";

// A record, so a kind added to the bindings fails to compile until listed.
const kinds = Object.keys({
  camera: true,
  microphone: true,
  location: true,
  notifications: true,
  midiSysex: true,
  sensors: true,
  windowManagement: true,
  localFonts: true,
  fileReadWrite: true,
  clipboardRead: true,
  multipleDownloads: true,
} satisfies Record<PermissionKind, true>) as PermissionKind[];

describe("permissionTitle", () => {
  it("names the origin that asked and what it asked for", () => {
    expect(permissionTitle({ id: 1, kind: "camera", origin: "https://meet.example" })).toBe(
      "https://meet.example wants to use your camera",
    );
    expect(permissionTitle({ id: 2, kind: "multipleDownloads", origin: "http://localhost:8080" })).toBe(
      "http://localhost:8080 wants to download multiple files",
    );
  });

  it("puts the origin in as it is", () => {
    expect(permissionTitle({ id: 3, kind: "camera", origin: "https://$&.example" })).toBe(
      "https://$&.example wants to use your camera",
    );
  });
});

describe("permissionName", () => {
  it("has a name and an action for every kind", () => {
    expect(permissionName("location")).toBe("Location");
    for (const kind of kinds) {
      expect(permissionName(kind)).not.toBe(`permission.name.${kind}`);
      expect(permissionAction(kind)).not.toBe(`permission.ask.${kind}`);
    }
  });
});

describe("toDecision", () => {
  it("allows on Allow, blocks on Block, and dismisses on Escape", () => {
    expect(toDecision(true, false)).toBe("allow");
    expect(toDecision(false, false)).toBe("block");
    expect(toDecision(false, true)).toBe("dismiss");
  });
});
