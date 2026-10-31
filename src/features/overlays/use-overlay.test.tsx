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

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
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

  it("stays registered while it animates away, then releases", () => {
    act(() => root.render(<Floating open />));

    animating = true;
    act(() => root.render(<Floating open={false} />));
    act(() => vi.advanceTimersToNextFrame());
    expect(overlayStore.get()).toHaveLength(1);

    animating = false;
    act(() => vi.advanceTimersToNextFrame());
    expect(overlayStore.get()).toHaveLength(0);
  });

  it("gives up following an animation that never ends", () => {
    act(() => root.render(<Floating open />));

    animating = true;
    act(() => root.render(<Floating open={false} />));
    act(() => vi.advanceTimersByTime(1100));

    expect(overlayStore.get()).toHaveLength(0);
  });
});
