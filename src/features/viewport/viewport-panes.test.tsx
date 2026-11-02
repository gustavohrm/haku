import type { Tab } from "@bindings";
import { usePageDialog } from "@features/feedback/use-page-dialog";
import { TabStrip } from "@features/tabs/tab-strip";
import { Toolbar } from "@features/toolbar/toolbar";
import { commands } from "@ipc/commands";
import { useActiveTab } from "@ipc/hooks";
import { browserStore, EMPTY_STATE } from "@ipc/store";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";

import { ViewportPanes } from "./viewport-panes";

const { alert, dismiss, addressListeners } = vi.hoisted(() => ({
  alert: vi.fn(),
  dismiss: vi.fn(),
  addressListeners: new Set<(event: { payload: number }) => void>(),
}));
vi.mock("@bindings", () => ({
  events: {
    addressFocusRequested: () => ({
      listen: async (listener: (event: { payload: number }) => void) => {
        addressListeners.add(listener);
        return () => addressListeners.delete(listener);
      },
    }),
    findRequested: () => ({ listen: async () => () => {} }),
    findResult: () => ({ listen: async () => () => {} }),
    findEnded: () => ({ listen: async () => () => {} }),
  },
}));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ label: "window-1" }) }));
vi.mock("@ipc/commands", () => ({
  commands: {
    focusPane: vi.fn(),
    closeTab: vi.fn(),
    selectTab: vi.fn(),
    tabPreview: vi.fn(),
    setLayout: vi.fn(),
    answerDialog: vi.fn(),
    reloadTab: vi.fn(),
  },
}));
vi.mock("@features/feedback/feedback", () => ({ feedback: () => ({ dialog: { alert } }) }));
vi.mock("@features/settings/settings-page", () => ({
  SettingsPage: ({ url, tabId }: { url: string; tabId: number }) => (
    <input aria-label={`${tabId}:${url}`} defaultValue="" />
  ),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const tab = (id: number, extra: Partial<Tab> = {}): Tab => ({
  id,
  history: { entries: [{ url: "https://example.com", title: `Page ${id}`, favicon: null }], index: 0 },
  presence: { status: "live", slot: id },
  scroll: { x: 0, y: 0 },
  fixed: false,
  activeAt: 0,
  dialog: null,
  permission: null,
  audible: false,
  zoom: 100,
  relieved: false,
  restoring: false,
  ...extra,
});
let host: HTMLDivElement;
let root: Root;

it("discovers background dialogs on keyboard-focusable badges and presents only the selected member", async () => {
  const dialog = (id: number) => ({
    id,
    kind: "alert" as const,
    url: "https://example.com",
    message: `Dialog ${id}`,
    defaultText: "",
  });
  browserStore.set({
    ...browserStore.get(),
    active: 3,
    focusedViewport: "single",
    viewports: [{ id: "single", tab: 3 }],
    tabs: [tab(1, { dialog: dialog(1) }), tab(2, { dialog: dialog(2) }), tab(3)],
  });
  vi.mocked(commands.selectTab).mockImplementation(async (id) => {
    browserStore.set({
      ...browserStore.get(),
      active: id,
      focusedViewport: id === 1 ? "left" : "right",
      viewports: [
        { id: "left", tab: 1 },
        { id: "right", tab: 2 },
      ],
    });
    return { status: "ok", data: browserStore.get() };
  });
  function Window() {
    usePageDialog(useActiveTab());
    return (
      <>
        <TabStrip />
        <ViewportPanes />
      </>
    );
  }
  await act(async () => root.render(<Window />));
  expect(alert).not.toHaveBeenCalled();
  const badge = host.querySelector<HTMLButtonElement>('[aria-label="Dialog waiting: Page 1"]')!;
  expect(badge.tabIndex).toBe(0);
  await act(async () => {
    badge.focus();
    badge.click();
  });
  expect(alert).toHaveBeenCalledExactlyOnceWith("Dialog 1", expect.objectContaining({ backdropDismiss: false }));
  const other = host.querySelector<HTMLButtonElement>('[aria-label="Dialog waiting: Page 2"]')!;
  await act(async () => other.click());
  expect(dismiss).toHaveBeenCalledTimes(1);
  expect(alert).toHaveBeenLastCalledWith("Dialog 2", expect.objectContaining({ backdropDismiss: false }));
});
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => {});
  vi.mocked(commands.tabPreview).mockResolvedValue({ status: "ok", data: "data:image/png;base64,preview" });
  alert.mockReturnValue(Object.assign(new Promise(() => {}), { dismiss }));
  browserStore.set({
    ...EMPTY_STATE,
    tabs: [tab(1), tab(2)],
    viewports: [
      { id: "left", tab: 1 },
      { id: "right", tab: 2 },
    ],
    focusedViewport: "left",
    active: 1,
    splits: [{ tabs: [1, 2], ratio: 0.5 }],
  });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  browserStore.set(EMPTY_STATE);
  vi.unstubAllGlobals();
});

it("keeps explicit internal routes and their local state separate in each pane", async () => {
  const internal = (id: number) =>
    tab(id, {
      presence: { status: "internal" },
      history: { entries: [{ url: `haku://settings/${id}`, title: "", favicon: null }], index: 0 },
    });
  browserStore.set({ ...browserStore.get(), tabs: [internal(1), internal(2)] });
  await act(async () => root.render(<ViewportPanes />));
  const inputs = host.querySelectorAll("input");
  expect([...inputs].map((input) => input.getAttribute("aria-label"))).toEqual([
    "1:haku://settings/1",
    "2:haku://settings/2",
  ]);
  inputs[0]!.value = "left search";
  await act(async () => browserStore.set({ ...browserStore.get(), focusedViewport: "right", active: 2 }));
  expect(inputs[0]!.value).toBe("left search");
  expect(inputs[1]!.value).toBe("");
  expect([...host.querySelectorAll<HTMLElement>("[data-viewport]")].map((pane) => pane.dataset.covered)).toEqual([
    "true",
    "true",
  ]);
});

it("covers crashed and restoring panes independently and reloads the named crashed tab", async () => {
  browserStore.set({
    ...browserStore.get(),
    tabs: [tab(1, { presence: { status: "crashed", cause: "unknown" } }), tab(2, { restoring: true })],
  });
  await act(async () => root.render(<ViewportPanes />));
  const panes = host.querySelectorAll<HTMLElement>("[data-viewport]");
  await act(async () => panes[0]!.querySelector<HTMLButtonElement>("button")!.click());
  expect(commands.reloadTab).toHaveBeenCalledWith(1);
  expect(panes[0]!.textContent).toContain("This page crashed");
  expect(panes[1]!.querySelector("img")?.src).toContain("preview");
  expect([...panes].map((pane) => pane.dataset.covered)).toEqual(["true", "true"]);
  await act(async () => browserStore.set({ ...browserStore.get(), tabs: [browserStore.get().tabs[0]!, tab(2)] }));
  expect(host.querySelector('[data-viewport="right"]')?.getAttribute("data-covered")).toBe("false");
});

it("marks a waiting dialog accessibly and focuses its pane before presenting the window-modal dialog", async () => {
  browserStore.set({
    ...browserStore.get(),
    tabs: [
      tab(1),
      tab(2, { dialog: { id: 20, kind: "alert", url: "https://example.com", message: "Waiting", defaultText: "" } }),
    ],
  });
  vi.mocked(commands.selectTab).mockImplementation(async () => {
    browserStore.set({ ...browserStore.get(), active: 2, focusedViewport: "right" });
    return { status: "ok", data: browserStore.get() };
  });
  function Window() {
    usePageDialog(useActiveTab());
    return (
      <>
        <TabStrip />
        <ViewportPanes />
      </>
    );
  }
  await act(async () => root.render(<Window />));
  expect(alert).not.toHaveBeenCalled();
  const marker = host.querySelector<HTMLButtonElement>('[aria-label="Dialog waiting: Page 2"]')!;
  expect(marker).not.toBeNull();
  // jsdom does not lay out flex items; retain the constraints that bound this badge.
  expect(marker.classList.contains("min-w-0")).toBe(true);
  expect(marker.classList.contains("shrink")).toBe(true);
  expect(marker.querySelector(".truncate")?.textContent).toBe("Dialog waiting");
  expect(marker.closest("[data-viewport]")).toBeNull();
  await act(async () => marker.click());
  expect(commands.selectTab).toHaveBeenCalledWith(2);
  expect(alert).toHaveBeenCalledExactlyOnceWith(
    "Waiting",
    expect.objectContaining({ title: "example.com says", backdropDismiss: false }),
  );
  expect(host.querySelector('[aria-label="Dialog waiting: Page 2"]')).toBeNull();
  await act(async () => browserStore.set({ ...browserStore.get(), active: 1, focusedViewport: "left" }));
  expect(dismiss).toHaveBeenCalledTimes(1);
});

it("outlines only the focused paired pane and keeps each joined half selectable and closable", async () => {
  await act(async () =>
    root.render(
      <>
        <TabStrip />
        <ViewportPanes />
      </>,
    ),
  );
  expect(host.querySelectorAll(".haku-pane-focused")).toHaveLength(1);
  expect(host.querySelector(".haku-pane-focused")?.getAttribute("aria-label")).toContain("Page 1");
  expect(host.querySelectorAll('[aria-current="page"]')).toHaveLength(1);
  const halves = host.querySelectorAll<HTMLButtonElement>("ul button[aria-describedby]");
  expect(halves).toHaveLength(2);
  await act(async () => halves[1]!.click());
  expect(commands.selectTab).toHaveBeenCalledWith(2);
  await act(async () => host.querySelectorAll<HTMLButtonElement>('[aria-label="Close tab"]')[1]!.click());
  expect(commands.closeTab).toHaveBeenCalledWith(2);
  await act(async () => browserStore.set({ ...browserStore.get(), viewports: [{ id: "left", tab: 1 }] }));
  expect(host.querySelectorAll(".haku-pane-focused")).toHaveLength(0);
  // The surface takes focus for a covered page; only the split outline may mark it.
  const surface = host.querySelector<HTMLElement>("[data-viewport]");
  expect(surface?.classList).toContain("outline-none");
  expect(surface?.tabIndex).toBe(-1);
});

it("keeps the address focused when a new internal tab consumes a pending focus request", async () => {
  function Window() {
    return (
      <>
        <Toolbar tab={useActiveTab()} />
        <ViewportPanes />
      </>
    );
  }
  await act(async () => root.render(<Window />));
  await act(async () => addressListeners.forEach((listener) => listener({ payload: 3 })));
  const internal = tab(3, {
    presence: { status: "internal" },
    history: { entries: [{ url: "haku://new-tab", title: "", favicon: null }], index: 0 },
  });
  await act(async () =>
    browserStore.set({
      ...browserStore.get(),
      active: 3,
      tabs: [...browserStore.get().tabs, internal],
      focusedViewport: "left",
      viewports: [{ id: "left", tab: 3 }],
    }),
  );
  expect(document.activeElement).toBe(host.querySelector('[aria-label="Search or enter address"]'));
});

it("shows a fullscreen pane alone, keeping its partner's surface to measure as hidden", async () => {
  browserStore.set({ ...browserStore.get(), fullscreen: 2 });
  await act(async () => root.render(<ViewportPanes />));

  expect(host.querySelector(".haku-split")).toBeNull();
  expect(host.querySelector('[role="separator"]')).toBeNull();
  const left = host.querySelector('[data-viewport="left"]')!.closest("section")!;
  const right = host.querySelector('[data-viewport="right"]')!.closest("section")!;
  expect(left.hidden).toBe(true);
  expect(right.hidden).toBe(false);
});
