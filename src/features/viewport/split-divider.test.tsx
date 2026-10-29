import { commands } from "@ipc/commands";
// oxlint-disable no-await-in-loop -- Gestures and their assertions must run in order.
import { act, createRef } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import { SplitDivider, splitLimits, clampSplitRatio } from "./split-divider";

vi.mock("@ipc/commands", () => ({ commands: { setSplitRatio: vi.fn() } }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("clamps to the ratio range and 240 pixels per pane, using equal widths below 484", () => {
  expect(clampSplitRatio(0.1, 1204)).toBe(0.2);
  expect(clampSplitRatio(0.9, 1204)).toBe(0.8);
  expect(clampSplitRatio(0.2, 604)).toBe(0.4);
  expect(clampSplitRatio(0.8, 604)).toBe(0.6);
  expect(clampSplitRatio(0.8, 483)).toBe(0.5);
  expect(splitLimits(483)).toEqual({ min: 0.5, max: 0.5 });
});

it("supports five-point steps, limits and equal widths with accessible separator values", async () => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 1204, left: 0 } as DOMRect);
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
          names={["One", "Two"]}
        />
      </div>,
    );
  try {
    await act(async () => render(0.5));
    const divider = host.querySelector('[role="separator"]')!;
    expect(divider.getAttribute("aria-label")).toContain("One");
    expect(divider.getAttribute("aria-label")).toContain("Two");
    expect(divider.getAttribute("aria-orientation")).toBe("vertical");
    for (const [key, result] of [
      ["ArrowRight", 0.55],
      ["ArrowLeft", 0.45],
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
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 1204, left: 0 } as DOMRect);
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
    const pointer = async (type: string, x: number) =>
      act(async () => divider.dispatchEvent(new MouseEvent(type, { clientX: x, button: 0, bubbles: true })));
    vi.mocked(commands.setSplitRatio).mockClear();
    await pointer("pointerdown", 600);
    await pointer("pointermove", 840);
    expect(capture).toHaveBeenCalledTimes(1);
    expect(commands.setSplitRatio).not.toHaveBeenCalled();
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.7");
    await act(async () => divider.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.5");
    await pointer("pointerup", 840);
    expect(commands.setSplitRatio).not.toHaveBeenCalled();
    await pointer("pointerdown", 600);
    await pointer("pointermove", 840);
    await pointer("pointerup", 840);
    await pointer("pointerup", 840);
    expect(commands.setSplitRatio).toHaveBeenCalledExactlyOnceWith(1, 0.7);
    vi.mocked(commands.setSplitRatio).mockClear();
    for (const [revision, assignments] of [
      [2, "left:1,right:2"],
      [2, "left:1,right:3"],
    ] as const) {
      await pointer("pointerdown", 600);
      await pointer("pointermove", 840);
      await act(async () => render(revision, assignments));
      await pointer("pointerup", 840);
      expect(commands.setSplitRatio).not.toHaveBeenCalled();
      expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.5");
    }
  } finally {
    await act(async () => root.unmount());
  }
});

it("disables resizing below 484 pixels without committing away the preferred ratio", async () => {
  let width = 1204;
  let resize!: () => void;
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
  const ref = createRef<HTMLDivElement>();
  const host = document.createElement("div");
  const root = createRoot(host);
  try {
    await act(async () =>
      root.render(
        <div ref={ref}>
          <SplitDivider
            containerRef={ref}
            ratio={0.7}
            tabId={1}
            revision={1}
            assignments="left:1,right:2"
            names={["One", "Two"]}
          />
        </div>,
      ),
    );
    const divider = host.querySelector('[role="separator"]')!;
    vi.mocked(commands.setSplitRatio).mockClear();
    await act(async () => divider.dispatchEvent(new MouseEvent("pointerdown", { button: 0, bubbles: true })));
    width = 483;
    await act(async () => resize());
    expect(divider.getAttribute("aria-disabled")).toBe("true");
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.5");
    await act(async () => divider.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })));
    await act(async () => divider.dispatchEvent(new MouseEvent("pointerup", { button: 0, bubbles: true })));
    expect(commands.setSplitRatio).not.toHaveBeenCalled();
    width = 1204;
    await act(async () => resize());
    expect(ref.current!.style.getPropertyValue("--split-ratio")).toBe("0.7");
    expect(divider.getAttribute("aria-disabled")).toBeNull();
  } finally {
    await act(async () => root.unmount());
  }
});
