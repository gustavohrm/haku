import type { ContextMenuRequested, Tab } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { state, listeners, commands } = vi.hoisted(() => ({
  state: {
    tabs: [] as Tab[],
    viewports: [
      { id: "left", tab: 7 },
      { id: "right", tab: 9 },
    ],
  },
  listeners: new Set<(event: { payload: ContextMenuRequested }) => void>(),
  commands: { reloadTab: vi.fn(), answerContextMenu: vi.fn(), focusPane: vi.fn() },
}));
vi.mock("@bindings", () => ({
  events: {
    contextMenuRequested: () => ({
      listen: async (listener: (event: { payload: ContextMenuRequested }) => void) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
    }),
  },
}));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ label: "window-1" }) }));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({ useBrowserState: () => state }));

import { paneGeometryStore } from "@features/viewport/use-chrome-layout";

import { closeMenu, useOpenMenu } from "./store";
import { usePageMenu } from "./use-page-menu";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tab = (id: number): Tab => ({
  id,
  history: { entries: [{ url: "https://example.com/", title: "", favicon: null }], index: 0 },
  presence: { status: "live", slot: id },
  scroll: { x: 0, y: 0 },
  fixed: false,
  activeAt: 0,
  dialog: null,
  audible: false,
  relieved: false,
  restoring: false,
});
const request = (id: number): ContextMenuRequested => ({
  request: 42,
  tab: id,
  x: 1,
  y: 2,
  items: [],
  target: {
    kind: "page",
    pageUrl: "https://example.com/",
    linkUrl: null,
    linkText: null,
    sourceUrl: null,
    selection: null,
    editable: false,
  },
});

function PageMenu({ focused }: { focused: Tab }) {
  usePageMenu(focused);
  const menu = useOpenMenu();
  return (
    <>
      {menu?.items.map(
        (item) =>
          "onSelect" in item && (
            <button key={item.id} data-action={item.id} onClick={item.onSelect}>
              {item.id}
            </button>
          ),
      )}
    </>
  );
}

describe("page menu targeting", () => {
  let container: HTMLDivElement;
  let root: Root;
  beforeEach(() => {
    vi.clearAllMocks();
    state.tabs = [tab(7), tab(9)];
    state.viewports = [
      { id: "left", tab: 7 },
      { id: "right", tab: 9 },
    ];
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });
  afterEach(async () => {
    await act(async () => {
      root.unmount();
      closeMenu();
    });
    container.remove();
  });
  const send = async (id: number) => {
    await act(async () => listeners.forEach((listener) => listener({ payload: request(id) })));
  };

  it("reloads the tab named by the page menu request", async () => {
    await act(async () => root.render(<PageMenu focused={state.tabs[0]!} />));
    await send(7);
    await act(async () => container.querySelector<HTMLButtonElement>('[data-action="reload"]')!.click());
    expect(commands.reloadTab).toHaveBeenCalledWith(7);
    await act(async () => root.render(<PageMenu focused={state.tabs[1]!} />));
    expect(commands.answerContextMenu).toHaveBeenCalledWith(42, null, false);
  });

  it("answers a stale request without opening a menu for the replacement tab", async () => {
    state.viewports = [{ id: "left", tab: 9 }];
    await act(async () => root.render(<PageMenu focused={state.tabs[1]!} />));
    await send(7);
    expect(container.querySelector("button")).toBeNull();
    expect(commands.answerContextMenu).toHaveBeenCalledWith(42, null, false);
    expect(commands.reloadTab).not.toHaveBeenCalled();
  });

  it("dismisses a page menu and completes its deferral when another tab takes focus", async () => {
    await act(async () => root.render(<PageMenu focused={state.tabs[0]!} />));
    await send(7);
    await act(async () => root.render(<PageMenu focused={state.tabs[1]!} />));
    expect(container.querySelector("button")).toBeNull();
    expect(commands.answerContextMenu).toHaveBeenCalledExactlyOnceWith(42, null, false);
  });

  it("opens a menu from the unfocused pane, focuses that pane and keeps the menu once focus catches up", async () => {
    await act(async () => root.render(<PageMenu focused={state.tabs[0]!} />));
    await send(9);
    expect(commands.focusPane).toHaveBeenCalledExactlyOnceWith(9);
    expect(container.querySelector("button")).not.toBeNull();
    await act(async () => root.render(<PageMenu focused={state.tabs[1]!} />));
    expect(container.querySelector("button")).not.toBeNull();
    expect(commands.answerContextMenu).not.toHaveBeenCalled();
  });
});

it("dismisses a page menu and completes its deferral when its source is reassigned", async () => {
  const container = document.createElement("div");
  const root = createRoot(container);
  state.tabs = [tab(7), tab(9)];
  state.viewports = [
    { id: "left", tab: 7 },
    { id: "right", tab: 9 },
  ];
  commands.answerContextMenu.mockClear();
  try {
    await act(async () => root.render(<PageMenu focused={state.tabs[0]!} />));
    await act(async () => listeners.forEach((listener) => listener({ payload: request(7) })));
    expect(container.querySelector("button")).not.toBeNull();
    state.viewports = [
      { id: "left", tab: 9 },
      { id: "right", tab: 7 },
    ];
    await act(async () => root.render(<PageMenu focused={state.tabs[0]!} />));
    expect(container.querySelector("button")).toBeNull();
    expect(commands.answerContextMenu).toHaveBeenCalledExactlyOnceWith(42, null, false);
  } finally {
    await act(async () => root.unmount());
  }
});

it("dismisses a page menu when the source pair leaves visibility", async () => {
  const container = document.createElement("div");
  const root = createRoot(container);
  state.tabs = [tab(7), tab(9), tab(11)];
  state.viewports = [
    { id: "left", tab: 7 },
    { id: "right", tab: 9 },
  ];
  commands.answerContextMenu.mockClear();
  try {
    await act(async () => root.render(<PageMenu focused={state.tabs[0]!} />));
    await act(async () => listeners.forEach((listener) => listener({ payload: request(7) })));
    expect(container.querySelector("button")).not.toBeNull();
    state.viewports = [{ id: "single", tab: 11 }];
    await act(async () => root.render(<PageMenu focused={state.tabs[2]!} />));
    expect(container.querySelector("button")).toBeNull();
    expect(commands.answerContextMenu).toHaveBeenCalledExactlyOnceWith(42, null, false);
  } finally {
    await act(async () => root.unmount());
  }
});

it("dismisses a page menu when its measured source geometry changes, preserving it for sibling-only changes", async () => {
  const container = document.createElement("div");
  const root = createRoot(container);
  state.tabs = [tab(7), tab(9)];
  state.viewports = [
    { id: "left", tab: 7 },
    { id: "right", tab: 9 },
  ];
  const left = { id: "left", rect: { x: 4, y: 80, width: 400, height: 600 }, radius: 8, covered: false };
  const right = { ...left, id: "right", rect: { ...left.rect, x: 408 } };
  paneGeometryStore.set([left, right]);
  commands.answerContextMenu.mockClear();
  try {
    await act(async () => root.render(<PageMenu focused={state.tabs[0]!} />));
    await act(async () => listeners.forEach((listener) => listener({ payload: request(7) })));
    await act(async () => paneGeometryStore.set([left, { ...right, covered: true }]));
    expect(container.querySelector("button")).not.toBeNull();
    await act(async () => paneGeometryStore.set([{ ...left, rect: { ...left.rect, width: 300 } }, right]));
    expect(container.querySelector("button")).toBeNull();
    expect(commands.answerContextMenu).toHaveBeenCalledExactlyOnceWith(42, null, false);
  } finally {
    await act(async () => root.unmount());
  }
});
