import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { info, dismiss } = vi.hoisted(() => {
  const dismiss = vi.fn();
  return { dismiss, info: vi.fn(() => ({ dismiss })) };
});
vi.mock("@features/feedback/feedback", () => ({ feedback: () => ({ info }) }));

import { useFullscreenNotice } from "./use-fullscreen-notice";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Notice({ url }: { url: string | null }) {
  useFullscreenNotice(url);
  return null;
}

describe("useFullscreenNotice", () => {
  let root: Root;

  beforeEach(() => {
    info.mockClear();
    dismiss.mockClear();
    root = createRoot(document.createElement("div"));
  });

  afterEach(async () => {
    await act(async () => root.unmount());
  });

  it("names the site once when its page goes fullscreen", async () => {
    await act(async () => root.render(<Notice url={null} />));
    expect(info).not.toHaveBeenCalled();

    await act(async () => root.render(<Notice url="https://www.youtube.com/watch?v=1" />));
    await act(async () => root.render(<Notice url="https://www.youtube.com/watch?v=1" />));

    expect(info).toHaveBeenCalledTimes(1);
    expect(info.mock.calls[0]).toEqual([
      expect.objectContaining({ title: "www.youtube.com is full screen", description: "Press Esc to exit." }),
    ]);
  });

  it("takes the notice away when fullscreen ends", async () => {
    await act(async () => root.render(<Notice url="https://a.test/" />));
    await act(async () => root.render(<Notice url={null} />));

    expect(dismiss).toHaveBeenCalledTimes(1);
  });

  it("speaks of the page when its address names no site", async () => {
    await act(async () => root.render(<Notice url="about:blank" />));

    expect(info.mock.calls[0]).toEqual([expect.objectContaining({ title: "This page is full screen" })]);
  });
});
