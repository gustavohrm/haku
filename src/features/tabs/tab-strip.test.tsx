import type { Tab } from "@bindings";
import { openMenu } from "@features/context-menu/store";
import { commands } from "@ipc/commands";
import { browserStore, EMPTY_STATE } from "@ipc/store";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

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

it.each([0, 1])("moves a background pair from half %i to the leading edge and cancels stale drags", async (half) => {
  vi.clearAllMocks();
  const initial = {
    ...EMPTY_STATE,
    tabs: [1, 2, 3, 4, 5].map(tab),
    active: 5,
    splits: [
      { tabs: [1, 2] as [number, number], ratio: 0.5 },
      { tabs: [3, 4] as [number, number], ratio: 0.5 },
    ],
  };
  browserStore.set(initial);
  const host = document.createElement("div");
  const root = createRoot(host);
  try {
    await act(async () => root.render(<TabStrip />));
    const units = host.querySelectorAll<HTMLElement>("[data-tab-unit]");
    vi.spyOn(units[0]!, "getBoundingClientRect").mockReturnValue({ left: 0, right: 200, width: 200 } as DOMRect);
    const start = () => {
      const event = new MouseEvent("dragstart", { bubbles: true });
      Object.defineProperty(event, "dataTransfer", { value: { setData: vi.fn() } });
      units[1]!.querySelectorAll("button[aria-describedby]")[half]!.dispatchEvent(event);
    };
    const over = () =>
      units[0]!.dispatchEvent(new MouseEvent("dragover", { bubbles: true, cancelable: true, clientX: 10 }));
    await act(async () => {
      start();
      over();
    });
    expect(host.querySelector('[data-drop-gap="0"]')).not.toBeNull();
    await act(async () => units[0]!.dispatchEvent(new MouseEvent("drop", { bubbles: true })));
    expect(commands.reorderTab).toHaveBeenCalledExactlyOnceWith(3, 0);
    vi.mocked(commands.reorderTab).mockClear();
    await act(async () => {
      start();
      over();
    });
    await act(async () => browserStore.set({ ...initial, splits: [] }));
    expect(host.querySelector("[data-drop-gap]")).toBeNull();
    await act(async () =>
      host.querySelector("[data-tab-unit]")!.dispatchEvent(new MouseEvent("drop", { bubbles: true })),
    );
    expect(commands.reorderTab).not.toHaveBeenCalled();
  } finally {
    await act(async () => root.unmount());
    browserStore.set(EMPTY_STATE);
  }
});

it("renders foreground and background joined pairs with individual selection", async () => {
  browserStore.set({
    ...EMPTY_STATE,
    tabs: [1, 2, 3, 4, 5].map(tab),
    active: 1,
    splits: [
      { tabs: [1, 2], ratio: 0.5 },
      { tabs: [3, 4], ratio: 0.7 },
    ],
  });
  const host = document.createElement("div");
  const root = createRoot(host);
  try {
    await act(async () => root.render(<TabStrip />));
    expect(host.querySelectorAll("[data-tab-unit]")).toHaveLength(3);
    expect(host.querySelectorAll('[aria-label="Paired tabs"]')).toHaveLength(2);
    const select = host.querySelectorAll<HTMLButtonElement>("button[aria-describedby]");
    expect(select).toHaveLength(4);
    await act(async () => select[3]!.click());
    expect(commands.selectTab).toHaveBeenCalledWith(4);
    for (const half of [2, 3]) {
      // oxlint-disable-next-line no-await-in-loop -- Each menu replaces the preceding one.
      await act(async () => select[half]!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true })));
      const request = vi.mocked(openMenu).mock.lastCall![0];
      expect(request.items.find((item) => item.id === "separate")).toMatchObject({ label: "Separate tabs" });
      expect(request.items.find((item) => item.id === "split")).toMatchObject({ disabled: true });
    }
  } finally {
    await act(async () => root.unmount());
    browserStore.set(EMPTY_STATE);
  }
});

it.each([0, 1])("drags half %i as the whole pair and exposes only unit gaps", async (half) => {
  vi.clearAllMocks();
  browserStore.set({
    ...EMPTY_STATE,
    tabs: [1, 2, 3, 4, 5].map(tab),
    active: 5,
    splits: [
      { tabs: [1, 2], ratio: 0.5 },
      { tabs: [3, 4], ratio: 0.5 },
    ],
  });
  const host = document.createElement("div");
  const root = createRoot(host);
  try {
    await act(async () => root.render(<TabStrip />));
    const units = host.querySelectorAll<HTMLElement>("[data-tab-unit]");
    expect(units).toHaveLength(3);
    vi.spyOn(units[1]!, "getBoundingClientRect").mockReturnValue({ left: 200, right: 400, width: 200 } as DOMRect);
    const start = new MouseEvent("dragstart", { bubbles: true });
    Object.defineProperty(start, "dataTransfer", { value: { setData: vi.fn(), effectAllowed: "" } });
    await act(async () => units[0]!.querySelectorAll("button[aria-describedby]")[half]!.dispatchEvent(start));
    await act(async () =>
      units[1]!.dispatchEvent(new MouseEvent("dragover", { bubbles: true, cancelable: true, clientX: 350 })),
    );
    expect(host.querySelector('[data-drop-gap="4"]')).not.toBeNull();
    await act(async () => units[1]!.dispatchEvent(new MouseEvent("drop", { bubbles: true, clientX: 350 })));
    expect(commands.reorderTab).toHaveBeenCalledExactlyOnceWith(1, 2);
  } finally {
    await act(async () => root.unmount());
    browserStore.set(EMPTY_STATE);
  }
});
