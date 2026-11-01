import { act, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { overlayStore } from "./registry";
import { useOverlay } from "./use-overlay";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;
/** Whether the overlay's element reports a running animation. */
let animating = false;
/** What the last ResizeObserver watches, and how to tell it something resized. */
let observed: Element[] = [];
let resized: () => void = () => {};

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: () => void) {
        resized = callback;
        observed = [];
      }
      observe(element: Element) {
        observed.push(element);
      }
      disconnect() {}
    },
  );
  Element.prototype.getAnimations = function () {
    return animating ? [{} as Animation] : [];
  };
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  animating = false;
  delete (Element.prototype as Partial<Element>).getAnimations;
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function Floating({ open }: { open: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  useOverlay(ref, open);
  return <div ref={ref} />;
}

describe("useOverlay", () => {
  it("registers while open and releases at once when nothing animates", () => {
    act(() => root.render(<Floating open />));
    expect(overlayStore.get()).toHaveLength(1);

    act(() => root.render(<Floating open={false} />));
    expect(overlayStore.get()).toHaveLength(0);
  });

  it("stays registered while it animates away, following it, then releases", () => {
    act(() => root.render(<Floating open />));
    const element = host.firstElementChild!;

    animating = true;
    act(() => root.render(<Floating open={false} />));
    element.getBoundingClientRect = () => new DOMRect(-40, 0, 200, 30);
    act(() => vi.advanceTimersToNextFrame());
    expect(overlayStore.get()).toHaveLength(1);
    expect(overlayStore.get()[0]?.rect.x).toBe(-40);

    animating = false;
    act(() => vi.advanceTimersToNextFrame());
    expect(overlayStore.get()).toHaveLength(0);
  });

  it("follows its parent, which moves it without resizing it", () => {
    act(() => root.render(<Floating open />));
    const element = host.firstElementChild!;
    expect(observed).toContain(host);

    element.getBoundingClientRect = () => new DOMRect(120, 0, 200, 30);
    act(() => resized());
    expect(overlayStore.get()[0]?.rect.x).toBe(120);
  });

  it("gives up following an animation that never ends", () => {
    act(() => root.render(<Floating open />));

    animating = true;
    act(() => root.render(<Floating open={false} />));
    act(() => vi.advanceTimersByTime(1100));

    expect(overlayStore.get()).toHaveLength(0);
  });
});
