import type { Extension, Settings } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const { openMenu, updateSettings, settings } = vi.hoisted(() => ({
  openMenu: vi.fn(),
  updateSettings: vi.fn(),
  settings: { pinnedExtensions: ["a", "b", "c"], disabledExtensions: [] as string[], keptSites: [] },
}));

const extension = (id: string): Extension => ({
  id,
  name: `Extension ${id}`,
  popup: `chrome-extension://${id}/popup.html`,
  icon: null,
  version: "1.0",
  folder: id,
  options: null,
});

vi.mock("@ipc/commands", () => ({ commands: { openExtensionPopup: vi.fn(), openTab: vi.fn() } }));
vi.mock("@ipc/store", async (actual) => ({ ...(await actual<object>()), updateSettings }));
vi.mock("@features/context-menu/store", () => ({ openMenu }));
vi.mock("@ipc/hooks", () => ({
  useExtensions: () => ({ installed: ["a", "b", "c"].map(extension), failed: [], error: null }),
  useSettings: () => settings,
}));

import { ExtensionsBar, placePanel } from "./extensions-bar";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  openMenu.mockReset();
  updateSettings.mockReset();
  vi.stubGlobal("PointerEvent", MouseEvent);
  HTMLElement.prototype.setPointerCapture = () => {};
  HTMLElement.prototype.releasePointerCapture = () => {};
  HTMLElement.prototype.hasPointerCapture = () => true;
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<ExtensionsBar tab={1} />));
  const group = host.querySelector<HTMLElement>('[role="group"]')!;
  vi.spyOn(group, "getBoundingClientRect").mockReturnValue(rect(0, 96));
  for (const [index, id] of ["a", "b", "c"].entries()) {
    vi.spyOn(pin(id), "getBoundingClientRect").mockReturnValue(rect(index * 32, 32));
  }
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  for (const name of ["setPointerCapture", "releasePointerCapture", "hasPointerCapture"]) {
    Reflect.deleteProperty(HTMLElement.prototype, name);
  }
});

function rect(left: number, width: number): DOMRect {
  return { left, right: left + width, width, top: 0, bottom: 32, height: 32 } as DOMRect;
}

function pin(id: string) {
  return host.querySelector<HTMLButtonElement>(`[data-pin="${id}"]`)!;
}

async function pointer(target: Element, type: string, x: number, y = 16) {
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 });
  Object.defineProperties(event, { pointerId: { value: 1 }, isPrimary: { value: true } });
  await act(async () => target.dispatchEvent(event));
}

/** The pinned order the last settings update saved. */
function saved() {
  const change = updateSettings.mock.calls.at(-1)![0] as (current: Settings) => Settings;
  return change(settings as unknown as Settings).pinnedExtensions;
}

it("moves a pin to where it is dropped", async () => {
  await pointer(pin("a"), "pointerdown", 16);
  await pointer(pin("a"), "pointermove", 70);
  await pointer(pin("a"), "pointerup", 90);
  expect(saved()).toEqual(["b", "c", "a"]);
});

it("keeps the order for a click, a drop off the toolbar on any side, or Escape", async () => {
  await pointer(pin("a"), "pointerdown", 16);
  await pointer(pin("a"), "pointerup", 18);

  await pointer(pin("a"), "pointerdown", 16);
  await pointer(pin("a"), "pointermove", 90, 200);
  await pointer(pin("a"), "pointerup", 90, 200);

  await pointer(pin("a"), "pointerdown", 16);
  await pointer(pin("a"), "pointermove", 400);
  await pointer(pin("a"), "pointerup", 400);

  await pointer(pin("a"), "pointerdown", 16);
  await pointer(pin("a"), "pointermove", 90);
  await act(async () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
  await pointer(pin("a"), "pointerup", 90);

  expect(updateSettings).not.toHaveBeenCalled();
});

it("does not open a popup for a press dragged away vertically", async () => {
  const { commands } = await import("@ipc/commands");
  await pointer(pin("a"), "pointerdown", 16);
  await pointer(pin("a"), "pointermove", 16, 200);
  await pointer(pin("a"), "pointerup", 16, 200);
  await act(async () => pin("a").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 })));
  expect(commands.openExtensionPopup).not.toHaveBeenCalled();
  expect(updateSettings).not.toHaveBeenCalled();
});

it("moves a pin from its context menu", async () => {
  await act(async () => pin("b").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true })));
  const items = openMenu.mock.calls[0]![0].items as { id: string; disabled?: boolean; onSelect?: () => void }[];
  items.find((item) => item.id === "left")!.onSelect!();
  expect(saved()).toEqual(["b", "a", "c"]);
  items.find((item) => item.id === "right")!.onSelect!();
  expect(saved()).toEqual(["a", "c", "b"]);

  await act(async () => pin("a").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true })));
  const first = openMenu.mock.calls[1]![0].items as { id: string; disabled?: boolean }[];
  expect(first.find((item) => item.id === "left")!.disabled).toBe(true);
});

it("opens a panel drawn with the overlay recipe, with no heading repeating its button", async () => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="Extensions"]')!.click());

  const panel = document.querySelector<HTMLElement>('section[aria-label="Extensions"]')!;
  expect(panel.classList).toContain("haku-overlay");
  expect(panel.classList).toContain("p-1");
  expect(panel.querySelector("h2")).toBeNull();
  const rows = [...panel.querySelectorAll(".haku-row")];
  expect(rows.map((row) => row.textContent)).toEqual([
    "Extension a",
    "Extension b",
    "Extension c",
    "Manage extensions",
  ]);
  for (const row of rows) {
    expect(row.classList).toContain("min-h-8");
  }
});

it("hangs the panel under its button, and inside the window from the foot of the column", () => {
  // Across the top: under the button, right edges aligned.
  // Each is held to the room on its side, a gap short of the window's edge, so a long list scrolls.
  expect(placePanel({ left: 1150, right: 1182, top: 34, bottom: 62 }, 1280, 800)).toEqual({
    top: 68,
    right: 98,
    maxHeight: 726,
  });
  // At the bottom left: above the button, from its left edge.
  expect(placePanel({ left: 139, right: 171, top: 760, bottom: 792 }, 1280, 800)).toEqual({
    bottom: 46,
    left: 139,
    maxHeight: 748,
  });
});
