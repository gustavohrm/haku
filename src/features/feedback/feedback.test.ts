import { overlayStore } from "@features/overlays/registry";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { feedback } from "./feedback";

vi.mock("@ipc/result", () => ({ HakuFailure: class extends Error {}, describeError: String }));

let frames: FrameRequestCallback[] = [];
const flushFrame = () => {
  const pending = frames;
  frames = [];
  pending.forEach((frame) => frame(0));
};
const boxAt = (element: Element, x: number, y: number, width: number, height: number) => {
  element.getBoundingClientRect = () => DOMRect.fromRect({ x, y, width, height });
};

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.stubGlobal("requestAnimationFrame", (frame: FrameRequestCallback) => frames.push(frame));
  vi.stubGlobal("cancelAnimationFrame", () => {});
});

afterEach(() => vi.unstubAllGlobals());

it("registers each toast where it is painted, with its corners, following it out of its stack", async () => {
  feedback().info({ title: "One", duration: 0 });
  feedback().info({ title: "Two", duration: 0 });
  await Promise.resolve();

  const container = document.querySelector<HTMLElement>("[data-haku-feedback]")!;
  const stack = [...container.children].find((child) => !(child instanceof HTMLDialogElement))!;
  const toasts = [...stack.children] as HTMLElement[];
  expect(toasts).toHaveLength(2);
  // jsdom does no layout: give the stack and its toasts the boxes a browser would.
  boxAt(stack, 600, 400, 300, 130);
  toasts.forEach((toast, index) => {
    toast.style.borderTopLeftRadius = "12px";
    boxAt(toast, 620, 400 + index * 70, 280, 60);
  });

  await Promise.resolve();
  flushFrame();
  expect(overlayStore.get()).toEqual([
    { rect: { x: 620, y: 400, width: 280, height: 60 }, radius: 12 },
    { rect: { x: 620, y: 470, width: 280, height: 60 }, radius: 12 },
  ]);

  // Sliding out past the stack's edge, with no change to the DOM.
  boxAt(toasts[1]!, 820, 470, 280, 60);
  flushFrame();
  expect(overlayStore.get()[1]).toEqual({ rect: { x: 820, y: 470, width: 80, height: 60 }, radius: 12 });

  toasts.forEach((toast) => toast.remove());
  await Promise.resolve();
  flushFrame();
  expect(overlayStore.get()).toEqual([]);
  flushFrame();
  expect(frames).toEqual([]);
});
