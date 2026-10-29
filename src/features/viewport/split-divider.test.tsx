import { commands } from "@ipc/commands";
import { EMPTY_STATE } from "@ipc/store";
// oxlint-disable no-await-in-loop -- Gestures and their assertions must run in order.
import { act, createRef } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { SplitDivider, splitLimits, clampSplitRatio } from "./split-divider";

vi.mock("@ipc/commands", () => ({ commands: { setSplitRatio: vi.fn() } }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
beforeEach(() => {
  vi.mocked(commands.setSplitRatio).mockReset();
  vi.mocked(commands.setSplitRatio).mockResolvedValue({ status: "ok", data: EMPTY_STATE });
});

async function pointer(target: Element, type: string, x = 0, id = 1) {
  const event = new MouseEvent(type, { clientX: x, button: 0, bubbles: true });
  Object.defineProperty(event, "pointerId", { value: id });
  await act(async () => target.dispatchEvent(event));
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("clamps to the ratio range and 240 pixels per pane, using equal widths below 485", () => {
  expect(clampSplitRatio(0.1, 1205)).toBe(0.2);
  expect(clampSplitRatio(0.9, 1205)).toBe(0.8);
  expect(clampSplitRatio(0.2, 605)).toBe(0.4);
  expect(clampSplitRatio(0.8, 605)).toBe(0.6);
  expect(clampSplitRatio(0.8, 484)).toBe(0.5);
  expect(splitLimits(484)).toEqual({ min: 0.5, max: 0.5 });
});

it("supports five-point steps, limits and equal widths with accessible separator values", async () => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 1205, left: 0 } as DOMRect);
  const ref = createRef<HTMLDivElement>();
  const host = document.createElement("div");
  const root = createRoot(host);
  const render = (ratio: number, revision = 1) =>
    root.render(
      <div ref={ref}>
        <SplitDivider
          containerRef={ref}
          ratio={ratio}
          tabId={1}
          revision={revision}
          assignments="left:1,right:2"
          names={["One {right}", "Two"]}
        />
      </div>,
    );
  try {
    await act(async () => render(0.5));
    const divider = host.querySelector('[role="separator"]')!;
    expect(divider.getAttribute("aria-label")).toContain("One {right}");
    expect(divider.getAttribute("aria-label")).toContain("Two");
    expect(divider.getAttribute("aria-orientation")).toBe("vertical");
    for (const [key, result] of [
      ["ArrowRight", 0.55],
      ["ArrowLeft", 0.5],
      ["Home", 0.2],
      ["End", 0.8],
      ["Enter", 0.5],
    ] as const) {
      vi.mocked(commands.setSplitRatio).mockClear();
      await act(async () => divider.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true })));
      expect(commands.setSplitRatio).toHaveBeenCalledExactlyOnceWith(1, result);
    }
  } finally {
    await act(async () => root.unmount());
  }
});

it("captures a drag, restores geometry on cancel and commits only once on release", async () => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 1205, left: 0 } as DOMRect);
  const capture = vi.fn();
  vi.stubGlobal("PointerEvent", MouseEvent);
  HTMLElement.prototype.setPointerCapture = capture;
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  const ref = createRef<HTMLDivElement>();
  const host = document.createElement("div");
  const root = createRoot(host);
  const render = (revision = 1, assignments = "left:1,right:2") =>
    root.render(
      <div ref={ref}>
        <SplitDivider
          containerRef={ref}
          ratio={0.5}
          tabId={1}
          revision={revision}
          assignments={assignments}
          names={["One", "Two"]}
        />
      </div>,
    );
  try {
    await act(async () => render());
    const divider = host.querySelector('[role="separator"]')!;
    vi.mocked(commands.setSplitRatio).mockClear();
    await pointer(divider, "pointerdown", 600);
    await pointer(divider, "pointermove", 960, 2);
    await pointer(divider, "pointerup", 960, 2);
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.5");
    expect(commands.setSplitRatio).not.toHaveBeenCalled();
    await pointer(divider, "pointermove", 840);
    expect(capture).toHaveBeenCalledTimes(1);
    expect(commands.setSplitRatio).not.toHaveBeenCalled();
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.7");
    await act(async () => divider.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.5");
    await pointer(divider, "pointerup", 840);
    expect(commands.setSplitRatio).not.toHaveBeenCalled();
    await pointer(divider, "pointerdown", 600);
    await pointer(divider, "pointermove", 840);
    await pointer(divider, "pointerup", 840);
    await pointer(divider, "pointerup", 840);
    expect(commands.setSplitRatio).toHaveBeenCalledExactlyOnceWith(1, 0.7);
    vi.mocked(commands.setSplitRatio).mockClear();
    for (const [revision, assignments] of [
      [2, "left:1,right:2"],
      [2, "left:1,right:3"],
    ] as const) {
      await pointer(divider, "pointerdown", 600);
      await pointer(divider, "pointermove", 840);
      await act(async () => render(revision, assignments));
      await pointer(divider, "pointerup", 840);
      expect(commands.setSplitRatio).not.toHaveBeenCalled();
      expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.5");
    }
  } finally {
    await act(async () => root.unmount());
  }
});

it("coalesces observation without writes, retains drags across resize and restores the preferred ratio", async () => {
  let width = 1205;
  let resize!: () => void;
  const frames = new Map<number, FrameRequestCallback>();
  let nextFrame = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.set(++nextFrame, callback);
    return nextFrame;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
  const flush = async () =>
    act(async () => {
      const callbacks = [...frames.values()];
      frames.clear();
      callbacks.forEach((callback) => callback(0));
    });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: () => void) {
        resize = callback;
      }
      observe() {}
      disconnect() {}
    },
  );
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(() => ({ width, left: 0 }) as DOMRect);
  vi.stubGlobal("PointerEvent", MouseEvent);
  let isCaptured = false;
  HTMLElement.prototype.setPointerCapture = vi.fn(() => {
    isCaptured = true;
  });
  HTMLElement.prototype.hasPointerCapture = () => isCaptured;
  HTMLElement.prototype.releasePointerCapture = vi.fn(() => {
    isCaptured = false;
  });
  const ref = createRef<HTMLDivElement>();
  const host = document.createElement("div");
  const root = createRoot(host);
  try {
    await act(async () =>
      root.render(
        <div ref={ref}>
          <SplitDivider
            containerRef={ref}
            ratio={0.8}
            tabId={1}
            revision={1}
            assignments="left:1,right:2"
            names={["One", "Two"]}
          />
        </div>,
      ),
    );
    const divider = host.querySelector('[role="separator"]')!;
    const writes = vi.spyOn(ref.current!.style, "setProperty");
    vi.mocked(commands.setSplitRatio).mockClear();
    width = 605;
    resize();
    resize();
    resize();
    expect(writes).not.toHaveBeenCalled();
    expect(frames.size).toBe(1);
    await flush();
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.6");
    writes.mockClear();
    resize();
    await flush();
    expect(writes).not.toHaveBeenCalled();
    await pointer(divider, "pointerdown");
    await pointer(divider, "pointermove", 300);
    width = 1205;
    resize();
    await flush();
    expect(isCaptured).toBe(true);
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.5");
    await act(async () => divider.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.8");
    await pointer(divider, "pointerup");
    expect(commands.setSplitRatio).not.toHaveBeenCalled();
    await pointer(divider, "pointerdown");
    await pointer(divider, "pointermove", 840);
    width = 1005;
    resize();
    await flush();
    expect(isCaptured).toBe(true);
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.7");
    await pointer(divider, "pointerup");
    await pointer(divider, "pointerup");
    expect(commands.setSplitRatio).toHaveBeenCalledExactlyOnceWith(1, 0.7);
    vi.mocked(commands.setSplitRatio).mockClear();
    await pointer(divider, "pointerdown");
    width = 484;
    resize();
    await flush();
    expect(divider.getAttribute("aria-disabled")).toBe("true");
    expect(isCaptured).toBe(true);
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.5");
    await pointer(divider, "pointerup");
    expect(commands.setSplitRatio).toHaveBeenCalledExactlyOnceWith(1, 0.5);
    resize();
    expect(frames.size).toBe(1);
  } finally {
    await act(async () => root.unmount());
  }
  expect(frames.size).toBe(0);
});

async function pendingDivider() {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 1205, left: 0 } as DOMRect);
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  const ref = createRef<HTMLDivElement>();
  const host = document.createElement("div");
  const root = createRoot(host);
  const replies: ((result: Awaited<ReturnType<typeof commands.setSplitRatio>>) => void)[] = [];
  vi.mocked(commands.setSplitRatio).mockImplementation(() => new Promise((resolve) => replies.push(resolve)));
  const render = (ratio: number) =>
    root.render(
      <div ref={ref}>
        <SplitDivider
          containerRef={ref}
          ratio={ratio}
          tabId={1}
          revision={1}
          assignments="left:1,right:2"
          names={["One", "Two"]}
        />
      </div>,
    );
  await act(async () => render(0.5));
  return { ref, root, render, replies, divider: host.querySelector('[role="separator"]')! };
}

it("accumulates keyboard steps across pending echoes and restores a failed step", async () => {
  const { ref, root, render, replies, divider } = await pendingDivider();
  try {
    for (let i = 0; i < 2; i++) {
      await act(async () => divider.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
    }
    expect(vi.mocked(commands.setSplitRatio).mock.calls.map(([, ratio]) => ratio)).toEqual([0.55, expect.closeTo(0.6)]);
    await act(async () => render(0.55));
    await act(async () => replies[0]!({ status: "ok", data: EMPTY_STATE }));
    await act(async () => divider.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
    expect(commands.setSplitRatio).toHaveBeenLastCalledWith(1, expect.closeTo(0.65));
    await act(async () => replies[1]!({ status: "ok", data: EMPTY_STATE }));
    await act(async () => replies[2]!({ status: "error", error: { kind: "Tauri", message: "failed" } }));
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.55");
  } finally {
    await act(async () => root.unmount());
  }
});

it("restores committed geometry when releasing a drag fails", async () => {
  const { ref, root, replies, divider } = await pendingDivider();
  try {
    await pointer(divider, "pointerdown", 660);
    await pointer(divider, "pointermove", 840);
    await pointer(divider, "pointerup", 840);
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.7");
    await act(async () => replies[0]!({ status: "error", error: { kind: "Tauri", message: "failed" } }));
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.5");
    expect(divider.getAttribute("aria-valuenow")).toBe("50");
  } finally {
    await act(async () => root.unmount());
  }
});
