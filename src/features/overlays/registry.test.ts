import { afterEach, describe, expect, it, vi } from "vitest";

import { overlayStore, registerOverlay, unregisterOverlay } from "./registry";

const rect = (x: number, y: number) => ({ x, y, width: 10, height: 10 });

afterEach(() => {
  unregisterOverlay("a");
  unregisterOverlay("b");
});

describe("overlay registry", () => {
  it("publishes a registered overlay", () => {
    registerOverlay("a", rect(1, 2));

    expect(overlayStore.get()).toEqual([rect(1, 2)]);
  });

  it("replaces an overlay rather than adding a second entry for the same id", () => {
    registerOverlay("a", rect(1, 2));
    registerOverlay("a", rect(3, 4));

    expect(overlayStore.get()).toEqual([rect(3, 4)]);
  });

  it("keeps overlays from different sources side by side", () => {
    registerOverlay("a", rect(1, 1));
    registerOverlay("b", rect(2, 2));

    expect(overlayStore.get()).toHaveLength(2);
  });

  it("does not notify when an overlay is registered at the rectangle it already had", () => {
    registerOverlay("a", rect(1, 2));
    const listener = vi.fn();
    const stop = overlayStore.subscribe(listener);

    registerOverlay("a", rect(1, 2));

    expect(listener).not.toHaveBeenCalled();
    stop();
  });

  it("removes an overlay so the page reclaims that area", () => {
    registerOverlay("a", rect(1, 2));
    unregisterOverlay("a");

    expect(overlayStore.get()).toEqual([]);
  });

  it("ignores removal of an overlay that was never registered", () => {
    const listener = vi.fn();
    const stop = overlayStore.subscribe(listener);

    unregisterOverlay("never-registered");

    expect(listener).not.toHaveBeenCalled();
    stop();
  });
});
