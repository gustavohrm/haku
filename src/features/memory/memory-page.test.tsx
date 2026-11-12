import { type MemoryReport, type Tab } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const { state, commands } = vi.hoisted(() => ({
  state: { isPrivate: false, tabs: [] as Tab[], report: null as MemoryReport | null },
  commands: { memoryReport: vi.fn() },
}));
vi.mock("@bindings", async (original) => ({
  ...(await original<object>()),
  events: { memoryChanged: { listen: vi.fn(async () => () => {}) } },
}));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({ useBrowserState: () => ({ private: state.isPrivate, tabs: state.tabs }) }));
vi.mock("@features/feedback/feedback", () => ({ reportUnexpected: vi.fn() }));

import { MemoryPage } from "./memory-page";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tab = (id: number): Tab => ({
  id,
  history: {
    entries: [{ url: "https://example.com/", title: `Page ${id}`, favicon: "https://example.com/icon.png" }],
    index: 0,
  },
  presence: { status: "live", slot: 0 },
  fixed: false,
  activeAt: 0,
  dialog: null,
  permission: null,
  audible: false,
  zoom: 100,
  relieved: false,
  restoring: false,
  scroll: { x: 0, y: 0 },
});
const slot = (id: number, tabId: number | null) => ({
  slot: id,
  tab: tabId,
  position: null,
  bytes: 1024,
  loss: null,
  signals: [],
});
const report = (overrides: Partial<MemoryReport> = {}): MemoryReport => ({
  pressure: "normal",
  headroom: 0.5,
  budget: 1000,
  kept: 0,
  slots: [slot(0, 1)],
  unattributed: [],
  ...overrides,
});

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  state.isPrivate = false;
  state.tabs = [tab(1)];
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

async function render(shown: MemoryReport) {
  commands.memoryReport.mockResolvedValue({ status: "ok", data: shown });
  await act(async () => root.render(<MemoryPage />));
}

const row = (id: number) => host.querySelector(`[data-slot="${id}"]`)!;

it("shows a tab's icon, and the generic one in a private window", async () => {
  await render(report());
  expect(row(0).querySelector("img")).not.toBeNull();
  await act(async () => root.unmount());
  root = createRoot(host);
  state.isPrivate = true;
  await render(report());
  expect(row(0).querySelector("img")).toBeNull();
  expect(row(0).querySelector(".ic-globe")).not.toBeNull();
});

it("says a slot is free only when it holds no tab", async () => {
  await render(report({ slots: [slot(0, null), slot(1, 7)] }));
  expect(row(0).textContent).toContain("Free");
  expect(row(1).textContent).not.toContain("Free");
  expect(row(1).textContent).toContain("A tab in another window");
});

it("fills the budget meter when tabs are kept over a budget of nothing", async () => {
  await render(report({ budget: 0, kept: 2048 }));
  const fills = Array.from(host.querySelectorAll<HTMLElement>("[style]")).map((element) => element.style.transform);
  expect(fills[0]).toBe("scaleX(1)");
});
