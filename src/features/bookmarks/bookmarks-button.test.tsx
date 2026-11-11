import type { Bookmarks } from "@bindings";
import type { MenuItem } from "@features/overlays/menu-model";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { state, openMenu, requests } = vi.hoisted(() => ({
  state: { tree: null as Bookmarks | null },
  openMenu: vi.fn(() => () => {}),
  requests: { handler: null as (() => void) | null },
}));
vi.mock("@ipc/commands", () => ({ commands: {} }));
vi.mock("@ipc/hooks", () => ({ useBookmarks: () => state.tree }));
vi.mock("@features/bars/holds", () => ({ holdBar: () => () => {} }));
vi.mock("@features/context-menu/store", () => ({ openMenu }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({}) }));
vi.mock("@bindings", () => ({
  events: {
    bookmarksMenuRequested: () => ({
      listen: (handler: () => void) => {
        requests.handler = handler;
        return Promise.resolve(() => {
          requests.handler = null;
        });
      },
    }),
  },
}));

import { BookmarksButton } from "./bookmarks-button";
import { sampleTree } from "./test-tree";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("BookmarksButton", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.clearAllMocks();
    // In the column, inside the window, where jsdom would say nowhere.
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(new DOMRect(40, 500, 28, 28));
    state.tree = sampleTree();
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.restoreAllMocks();
  });

  const render = () => act(async () => root.render(<BookmarksButton tab={7} />));
  const lastMenu = () => (openMenu.mock.lastCall as unknown as [{ items: MenuItem[] }])[0];

  it("opens the bar's bookmarks, then Other bookmarks and Manage", async () => {
    await render();
    await act(async () => {
      container.querySelector("button")!.click();
    });
    expect(openMenu).toHaveBeenCalledOnce();
    // Dev builds add Inspect interface after these.
    expect(
      lastMenu()
        .items.map((entry) => entry.id)
        .slice(0, 6),
    ).toEqual(["folder:3", "link:8", "separator:other", "other", "separator:manage", "manage"]);
    expect(container.querySelector("button")!.getAttribute("aria-expanded")).toBe("true");
  });

  it("waits for a hidden floating column to slide in before placing the menu", async () => {
    vi.stubGlobal("requestAnimationFrame", (step: FrameRequestCallback) => setTimeout(() => step(0), 0));
    try {
      await render();
      const button = container.querySelector("button")!;
      // Off the window's left edge until a frame has passed, then in the column.
      const rect = vi.spyOn(button, "getBoundingClientRect").mockReturnValue(new DOMRect(-200, 500, 28, 28));
      await act(async () => requests.handler?.());
      rect.mockReturnValue(new DOMRect(40, 500, 28, 28));
      await act(async () => new Promise((resolve) => setTimeout(resolve, 10)));

      expect(openMenu).toHaveBeenCalledOnce();
      expect((openMenu.mock.lastCall as unknown as [{ at: { x: number } }])[0].at.x).toBe(40);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("opens the same menu when Ctrl+Shift+B asks for it, and stops listening once gone", async () => {
    await render();
    await act(async () => requests.handler?.());
    expect(openMenu).toHaveBeenCalledOnce();

    await act(async () => root.unmount());
    expect(requests.handler).toBeNull();
    root = createRoot(container);
  });
});
