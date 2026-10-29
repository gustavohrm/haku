import { overlayStore } from "@features/overlays/registry";
import { commands } from "@ipc/commands";
import { browserStore, EMPTY_STATE } from "@ipc/store";
import { act, useRef } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { useChromeLayout } from "./use-chrome-layout";

vi.mock("@bindings", () => ({ events: {} }));
vi.mock("@ipc/commands", () => ({ commands: { setLayout: vi.fn() } }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  vi.mocked(commands.setLayout).mockReset();
  vi.mocked(commands.setLayout).mockImplementation(async () => ({ status: "ok", data: browserStore.get() }));
  vi.useFakeTimers();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(callback, 0));
  vi.stubGlobal("cancelAnimationFrame", clearTimeout);
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  browserStore.set(EMPTY_STATE);
  overlayStore.set([]);
});

it("reports named measured panes and covers with the current assignment revision", async () => {
  const rect = { x: 4, y: 80, width: 900, height: 600 };
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(rect as DOMRect);
  browserStore.set({ ...EMPTY_STATE, viewports: [{ id: "pane", tab: 1 }], focusedViewport: "pane", layoutRevision: 7 });
  overlayStore.set([{ rect, radius: 3 }]);
  function Surface({ covered }: { covered: boolean }) {
    const ref = useRef<HTMLDivElement>(null);
    useChromeLayout(ref, covered);
    return <div ref={ref} style={{ borderTopLeftRadius: "12px" }} />;
  }
  const root = createRoot(document.createElement("div"));
  try {
    await act(async () => root.render(<Surface covered={false} />));
    await act(async () => vi.runAllTimers());
    expect(commands.setLayout).toHaveBeenLastCalledWith({
      revision: 7,
      viewports: [{ id: "pane", rect, covered: false, radius: 12 }],
      overlays: [{ rect, radius: 3 }],
    });
    await act(async () => root.render(<Surface covered />));
    await act(async () => vi.runAllTimers());
    expect(commands.setLayout).toHaveBeenLastCalledWith({
      revision: 7,
      viewports: [{ id: "pane", rect, covered: true, radius: 12 }],
      overlays: [{ rect, radius: 3 }],
    });
    await act(async () => browserStore.set({ ...browserStore.get(), layoutRevision: 8 }));
    await act(async () => vi.runAllTimers());
    expect(commands.setLayout).toHaveBeenLastCalledWith(expect.objectContaining({ revision: 8 }));
  } finally {
    await act(async () => root.unmount());
  }
});

it("recovers a stale startup snapshot using state returned by setLayout", async () => {
  // connect() has finished with an old snapshot before the first render.
  const stale = { ...EMPTY_STATE, viewports: [{ id: "1", tab: 1 }], focusedViewport: "1", layoutRevision: 1 };
  const current = { ...stale, layoutRevision: 2 };
  browserStore.set(stale);
  vi.mocked(commands.setLayout).mockResolvedValue({ status: "ok", data: current });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    x: 4,
    y: 80,
    width: 900,
    height: 600,
  } as DOMRect);
  function Surface() {
    const ref = useRef<HTMLDivElement>(null);
    useChromeLayout(ref, false);
    return <div ref={ref} />;
  }
  const root = createRoot(document.createElement("div"));
  try {
    await act(async () => root.render(<Surface />));
    await act(async () => vi.runAllTimers());
    await act(async () => vi.runAllTimers());

    expect(commands.setLayout).toHaveBeenNthCalledWith(1, expect.objectContaining({ revision: 1 }));
    expect(commands.setLayout).toHaveBeenNthCalledWith(2, expect.objectContaining({ revision: 2 }));
    expect(commands.setLayout).toHaveBeenCalledTimes(2);
    expect(browserStore.get()).toBe(current);
  } finally {
    await act(async () => root.unmount());
  }
});
