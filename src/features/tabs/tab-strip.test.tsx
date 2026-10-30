// oxlint-disable no-await-in-loop -- Each gesture and assertion runs in order.
import type { Tab } from "@bindings";
import { openMenu } from "@features/context-menu/store";
import { commands } from "@ipc/commands";
import { browserStore, EMPTY_STATE } from "@ipc/store";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { TabStrip } from "./tab-strip";

vi.mock("@ipc/commands", () => ({ commands: { reorderTab: vi.fn(), selectTab: vi.fn(), closeTab: vi.fn() } }));
vi.mock("@features/context-menu/store", () => ({ openMenu: vi.fn() }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const tab = (id: number): Tab => ({
  id,
  history: { entries: [{ url: "https://example.com", title: `Page ${id}`, favicon: null }], index: 0 },
  presence: { status: "discarded" },
  fixed: false,
  activeAt: 0,
  dialog: null,
  audible: false,
  relieved: false,
  restoring: false,
  scroll: { x: 0, y: 0 },
});
const initial = {
  ...EMPTY_STATE,
  tabs: [1, 2, 3, 4, 5].map(tab),
  active: 1,
  splits: [
    { tabs: [1, 2] as [number, number], ratio: 0.5 },
    { tabs: [3, 4] as [number, number], ratio: 0.7 },
  ],
};
let host: HTMLDivElement;
let root: ReturnType<typeof createRoot>;
const capture = vi.fn();
const release = vi.fn();

beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("PointerEvent", MouseEvent);
  HTMLElement.prototype.setPointerCapture = capture;
  HTMLElement.prototype.releasePointerCapture = release;
  HTMLElement.prototype.hasPointerCapture = () => true;
  browserStore.set(initial);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<TabStrip />));
  const units = host.querySelectorAll<HTMLElement>("[data-tab-unit]");
  vi.spyOn(units[0]!.parentElement!, "getBoundingClientRect").mockReturnValue(rect(0, 500));
  for (const [index, unit] of Array.from(units).entries()) {
    vi.spyOn(unit, "getBoundingClientRect").mockReturnValue(rect(index * 200, index === 2 ? 100 : 200));
  }
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  browserStore.set(EMPTY_STATE);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  for (const name of ["setPointerCapture", "releasePointerCapture", "hasPointerCapture"]) {
    Reflect.deleteProperty(HTMLElement.prototype, name);
  }
});

function rect(left: number, width: number): DOMRect {
  return { left, right: left + width, width, top: 0, bottom: 32, height: 32 } as DOMRect;
}

function member(index: number) {
  return host.querySelectorAll<HTMLButtonElement>("button[aria-describedby]")[index]!;
}

async function pointer(
  target: Element,
  type: string,
  x: number,
  options: { y?: number; button?: number; id?: number } = {},
) {
  const event = new MouseEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX: x,
    clientY: options.y ?? 16,
    button: options.button ?? 0,
  });
  Object.defineProperties(event, {
    pointerId: { value: options.id ?? 1 },
    isPrimary: { value: true },
  });
  await act(async () => target.dispatchEvent(event));
}

it.each([0, 1])("moves a background pair from half %i to the leading edge with pointer capture", async (half) => {
  const source = member(2 + half);
  await pointer(source, "pointerdown", 250 + half * 100);
  expect(capture).toHaveBeenCalledExactlyOnceWith(1);
  await pointer(source, "pointermove", -10);
  expect(host.querySelector('[data-drop-gap="0"]')).not.toBeNull();
  expect(commands.reorderTab).not.toHaveBeenCalled();
  await pointer(source, "pointerup", -10);
  expect(commands.reorderTab).toHaveBeenCalledExactlyOnceWith(3, 0);
  expect(release).toHaveBeenCalledWith(1);
  await act(async () => browserStore.set({ ...initial, tabs: [3, 4, 1, 2, 5].map(tab) }));
  await act(async () => source.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 })));
  expect(commands.selectTab).not.toHaveBeenCalled();
});

it.each([0, 1])("drags half %i as the whole pair and exposes only unit gaps", async (half) => {
  const source = member(half);
  await pointer(source, "pointerdown", 50 + half * 100);
  for (const [x, gap] of [
    [250, 2],
    [350, 4],
    [550, 5],
  ]) {
    await pointer(source, "pointermove", x!);
    expect(host.querySelector(`[data-drop-gap="${gap}"]`)).not.toBeNull();
    expect(host.querySelector('[data-drop-gap="3"]')).toBeNull();
  }
  await pointer(source, "pointerup", 350);
  expect(commands.reorderTab).toHaveBeenCalledExactlyOnceWith(1, 2);
});

it.each(["membership", "order"])("cancels stale pointer drags when %s changes", async (change) => {
  const source = member(2);
  await pointer(source, "pointerdown", 250);
  await pointer(source, "pointermove", 10);
  expect(host.querySelector("[data-drop-gap]")).not.toBeNull();
  await act(async () =>
    browserStore.set(
      change === "membership" ? { ...initial, splits: [] } : { ...initial, tabs: [5, 1, 2, 3, 4].map(tab) },
    ),
  );
  expect(host.querySelector("[data-drop-gap]")).toBeNull();
  expect(release).toHaveBeenCalledWith(1);
  await pointer(source, "pointerup", 10);
  expect(commands.reorderTab).not.toHaveBeenCalled();
  await act(async () =>
    host
      .querySelector<HTMLButtonElement>("[data-tab-drag-handle]")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 })),
  );
  expect(commands.selectTab).toHaveBeenCalledExactlyOnceWith(change === "order" ? 5 : 1);
});

it.each(["Escape", "pointercancel", "lostpointercapture"])("cancels a drag on %s", async (cancel) => {
  const source = member(2);
  await pointer(source, "pointerdown", 250);
  await pointer(source, "pointermove", 10);
  expect(host.querySelector("[data-drop-gap]")).not.toBeNull();
  if (cancel === "Escape") {
    await act(async () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  } else {
    await pointer(source, cancel, 10);
  }
  expect(host.querySelector("[data-drop-gap]")).toBeNull();
  await pointer(source, "pointerup", 10);
  expect(commands.reorderTab).not.toHaveBeenCalled();
  await act(async () =>
    host
      .querySelector<HTMLButtonElement>("[data-tab-drag-handle]")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 })),
  );
  expect(commands.selectTab).toHaveBeenCalledExactlyOnceWith(1);
});

it("keeps below-threshold clicks selecting the individual half", async () => {
  const source = member(3);
  await pointer(source, "pointerdown", 350);
  await pointer(source, "pointermove", 352);
  expect(host.querySelector("[data-drop-gap]")).toBeNull();
  await pointer(source, "pointerup", 352);
  await act(async () => source.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 })));
  expect(commands.selectTab).toHaveBeenCalledExactlyOnceWith(4);
  expect(commands.reorderTab).not.toHaveBeenCalled();
});

it("ignores other pointers and drops outside the strip or in the source gap", async () => {
  const source = member(2);
  await pointer(source, "pointerdown", 250);
  await pointer(source, "pointermove", 10, { id: 2 });
  await pointer(source, "pointerup", 10, { id: 2 });
  expect(host.querySelector("[data-drop-gap]")).toBeNull();
  await pointer(source, "pointermove", 10);
  expect(host.querySelector("[data-drop-gap]")).not.toBeNull();
  await pointer(source, "pointermove", 10, { y: 80 });
  expect(host.querySelector("[data-drop-gap]")).toBeNull();
  await pointer(source, "pointerup", 10, { y: 80 });
  expect(commands.reorderTab).not.toHaveBeenCalled();
  await pointer(source, "pointerdown", 250);
  await pointer(source, "pointermove", 350);
  await pointer(source, "pointerup", 350);
  expect(commands.reorderTab).not.toHaveBeenCalled();
});

it("retains individual close, middle-click, selection and menu actions on joined pairs", async () => {
  expect(host.querySelectorAll("[data-tab-unit]")).toHaveLength(3);
  expect(host.querySelectorAll('[aria-label="Paired tabs"]')).toHaveLength(2);
  const close = host.querySelectorAll<HTMLButtonElement>('button[aria-label="Close tab"]')[3]!;
  await pointer(close, "pointerdown", 390);
  await pointer(close, "pointermove", 10);
  await pointer(close, "pointerup", 10);
  await act(async () => close.click());
  expect(capture).not.toHaveBeenCalled();
  expect(commands.closeTab).toHaveBeenCalledExactlyOnceWith(4);
  await pointer(member(2), "pointerdown", 250, { button: 1 });
  await act(async () => member(2).dispatchEvent(new MouseEvent("auxclick", { button: 1, bubbles: true })));
  expect(commands.closeTab).toHaveBeenLastCalledWith(3);
  await act(async () => member(3).click());
  expect(commands.selectTab).toHaveBeenCalledWith(4);
  for (const half of [2, 3]) {
    await pointer(member(half), "pointerdown", 250, { button: 2 });
    await act(async () => member(half).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true })));
    const request = vi.mocked(openMenu).mock.lastCall![0];
    expect(request.items.find((item) => item.id === "separate")).toMatchObject({ label: "Separate tabs" });
    expect(request.items.find((item) => item.id === "split")).toMatchObject({ disabled: true });
  }
  expect(capture).not.toHaveBeenCalled();
  expect(commands.reorderTab).not.toHaveBeenCalled();
});

it("uses a generic icon in private windows even when state supplies a favicon", async () => {
  const tabs = [tab(1)];
  tabs[0]!.history.entries[0]!.favicon = "https://example.com/favicon.ico";
  await act(async () => browserStore.set({ ...initial, tabs, splits: [], private: false }));
  expect(host.querySelector("img")?.getAttribute("src")).toBe("https://example.com/favicon.ico");
  await act(async () => browserStore.set({ ...initial, tabs, splits: [], private: true }));
  expect(host.querySelector("img")).toBeNull();
  expect(host.querySelector(".ic-globe")).not.toBeNull();
});
