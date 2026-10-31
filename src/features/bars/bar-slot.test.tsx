import { overlayStore } from "@features/overlays/registry";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { BarSlot } from "./bar-slot";
import { barHoldStore, holdBar as hold } from "./holds";
import { HIDE_DELAY_MS, SHOW_DELAY_MS } from "./use-reveal";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const releases: (() => void)[] = [];
const holdBar = (content: "tabs" | "toolbar") => {
  const release = hold(content);
  releases.push(release);
  return release;
};
let host: HTMLDivElement;
let root: ReturnType<typeof createRoot>;

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("PointerEvent", MouseEvent);
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  releases.splice(0).forEach((release) => release());
  barHoldStore.set({ tabs: 0, toolbar: 0 });
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

async function render({ floating = true, isNeeded = false } = {}) {
  await act(async () =>
    root.render(
      <BarSlot edge="top" contents={["toolbar"]} floating={floating} isNeeded={isNeeded}>
        <input aria-label="Address" />
      </BarSlot>,
    ),
  );
}

const slot = () => host.querySelector<HTMLElement>(".haku-bar-slot")!;
const revealed = () => slot().dataset.revealed === "true";

async function pointer(type: "pointerover" | "pointerout", target: Element, related: Element | null = null) {
  await act(async () => target.dispatchEvent(new MouseEvent(type, { bubbles: true, relatedTarget: related })));
}

describe("BarSlot", () => {
  it("lays a docked bar out in place, with nothing to reveal", async () => {
    await render({ floating: false });

    expect(slot().dataset.revealed).toBeUndefined();
    expect(host.querySelector("[data-bar-edge]")).toBeNull();
  });

  it("hides a floating bar until something shows it", async () => {
    await render();

    expect(revealed()).toBe(false);
    expect(overlayStore.get()).toHaveLength(0);
  });

  it("shows once the pointer has rested at the window edge, and registers as an overlay", async () => {
    await render();

    await pointer("pointerover", host.querySelector("[data-bar-edge]")!);
    expect(revealed()).toBe(false);
    await act(async () => vi.advanceTimersByTime(SHOW_DELAY_MS));

    expect(revealed()).toBe(true);
    expect(overlayStore.get()).toHaveLength(1);
  });

  it("does not show for a pointer passing over the edge", async () => {
    await render();
    const edge = host.querySelector("[data-bar-edge]")!;

    await pointer("pointerover", edge);
    await act(async () => vi.advanceTimersByTime(SHOW_DELAY_MS - 1));
    await pointer("pointerout", edge, document.body);
    await act(async () => vi.advanceTimersByTime(SHOW_DELAY_MS));

    expect(revealed()).toBe(false);
    expect(overlayStore.get()).toHaveLength(0);
  });

  it("takes the pointer at once when it is already shown", async () => {
    await render({ isNeeded: true });
    const edge = host.querySelector("[data-bar-edge]")!;
    await pointer("pointerover", edge);

    await render();

    expect(revealed()).toBe(true);
  });

  it("hides a moment after the pointer leaves, not at once", async () => {
    await render();
    const edge = host.querySelector("[data-bar-edge]")!;
    await pointer("pointerover", edge);
    await act(async () => vi.advanceTimersByTime(SHOW_DELAY_MS));

    await pointer("pointerout", edge, document.body);
    expect(revealed()).toBe(true);
    await act(async () => vi.advanceTimersByTime(HIDE_DELAY_MS));

    expect(revealed()).toBe(false);
    expect(overlayStore.get()).toHaveLength(0);
  });

  it("stays when the pointer comes back before the delay ends", async () => {
    await render();
    const edge = host.querySelector("[data-bar-edge]")!;
    await pointer("pointerover", edge);
    await act(async () => vi.advanceTimersByTime(SHOW_DELAY_MS));
    await pointer("pointerout", edge, document.body);

    await pointer("pointerover", edge, document.body);
    await act(async () => vi.advanceTimersByTime(HIDE_DELAY_MS));

    expect(revealed()).toBe(true);
  });

  it("shows while something in it has focus, as the address field does on Ctrl+L", async () => {
    await render();
    const input = host.querySelector("input")!;

    await act(async () => input.focus());
    expect(revealed()).toBe(true);

    await act(async () => input.blur());
    expect(revealed()).toBe(false);
  });

  it("hides once the pointer leaves after its focused element is removed", async () => {
    await render();
    const input = host.querySelector("input")!;
    await act(async () => input.focus());
    await act(async () => input.remove());
    expect(revealed()).toBe(true);

    await pointer("pointerout", slot(), document.body);
    await act(async () => vi.advanceTimersByTime(HIDE_DELAY_MS));

    expect(revealed()).toBe(false);
  });

  it("keeps its contents when it switches between docked and floating", async () => {
    await render({ floating: false });
    const input = host.querySelector("input")!;
    input.value = "typed";

    await render();

    expect(host.querySelector("input")).toBe(input);
    expect(input.value).toBe("typed");
  });

  it("stays shown when it starts floating while focused", async () => {
    await render({ floating: false });
    await act(async () => host.querySelector("input")!.focus());

    await render();

    expect(revealed()).toBe(true);
  });

  it("stays shown when it starts floating under the pointer, without waiting", async () => {
    await render({ floating: false });
    await pointer("pointerover", host.querySelector("input")!);

    await render();

    expect(revealed()).toBe(true);
  });

  it("shows while a menu opened from it holds it", async () => {
    await render();

    const release = await act(async () => holdBar("toolbar"));
    expect(revealed()).toBe(true);

    await act(async () => release());
    expect(revealed()).toBe(false);
  });

  it("ignores a hold on content it does not have", async () => {
    await render();

    await act(async () => void holdBar("tabs"));

    expect(revealed()).toBe(false);
  });

  it("shows while something in it needs attention", async () => {
    await render({ isNeeded: true });

    expect(revealed()).toBe(true);
  });

  it("puts the bar shown last above the others", async () => {
    await act(async () =>
      root.render(
        <>
          <BarSlot edge="top" contents={["toolbar"]} floating isNeeded={false}>
            <input aria-label="Address" />
          </BarSlot>
          <BarSlot edge="side" contents={["tabs"]} floating isNeeded={false}>
            <button type="button">Tab</button>
          </BarSlot>
        </>,
      ),
    );
    const [top, side] = host.querySelectorAll<HTMLElement>(".haku-bar-slot");
    const layer = (element: HTMLElement) => Number(element.style.zIndex);

    await act(async () => holdBar("tabs"));
    await act(async () => holdBar("toolbar"));
    expect(layer(top!)).toBeGreaterThan(layer(side!));

    await act(async () => releases.splice(0).forEach((release) => release()));
    await act(async () => vi.advanceTimersByTime(HIDE_DELAY_MS));
    await act(async () => holdBar("tabs"));
    expect(layer(side!)).toBeGreaterThan(layer(top!));
  });
});
