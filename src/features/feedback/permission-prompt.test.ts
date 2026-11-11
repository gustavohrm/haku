import type { PermissionKind } from "@bindings";
import { describe, expect, it } from "vitest";

import { permissionAction, permissionName, permissionTitle, placePrompt, PROMPT_INSET } from "./permission-prompt";

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

describe("placePrompt", () => {
  const size = { width: 320, height: 120 };
  const bounds = { width: 1200, height: 800 };
  const box = (left: number, top: number, right: number, bottom: number) => ({ left, top, right, bottom });

  it("opens under the site menu's glyph when the toolbar runs across the top", () => {
    const anchor = { field: box(300, 8, 900, 40), glyph: box(312, 16, 328, 32), sidebar: null };
    expect(placePrompt(anchor, size, bounds)).toEqual({ x: 312 - PROMPT_INSET, y: 46 });
  });

  it("opens beside a sidebar on the left, over the page, so the tabs stay in view", () => {
    const sidebar = box(0, 0, 260, 800);
    const anchor = { field: box(8, 40, 252, 72), glyph: box(18, 48, 34, 64), sidebar };
    expect(placePrompt(anchor, size, bounds)).toEqual({ x: 266, y: 40 });
  });

  it("opens beside a sidebar on the right towards the page, not off the window", () => {
    const sidebar = box(940, 0, 1200, 800);
    const anchor = { field: box(948, 40, 1192, 72), glyph: box(958, 48, 974, 64), sidebar };
    expect(placePrompt(anchor, size, bounds)).toEqual({ x: 940 - 6 - 320, y: 40 });
  });

  it("keeps inside the window when the address bar is low in the sidebar", () => {
    const sidebar = box(0, 0, 260, 800);
    const anchor = { field: box(8, 760, 252, 792), glyph: box(18, 768, 34, 784), sidebar };
    expect(placePrompt(anchor, size, bounds).y).toBe(792 - 120);
  });

  it("keeps a top-bar prompt inside the window near its right edge", () => {
    const anchor = { field: box(1000, 8, 1190, 40), glyph: box(1100, 16, 1116, 32), sidebar: null };
    expect(placePrompt(anchor, size, bounds).x).toBe(1116 + PROMPT_INSET - 320);
  });

  it("goes to the top centre of the window with no site menu on screen", () => {
    expect(placePrompt(null, size, bounds)).toEqual({ x: 440, y: 6 });
  });
});
