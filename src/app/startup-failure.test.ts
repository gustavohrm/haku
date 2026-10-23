import { beforeEach, describe, expect, it, vi } from "vitest";

import { showStartupFailure } from "./startup-failure";

const close = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close }) }));

describe("showStartupFailure", () => {
  const root = () => document.querySelector<HTMLElement>("#root");

  beforeEach(() => {
    close.mockClear();
    document.body.innerHTML = '<div id="root"><div class="haku-splash"></div></div>';
    showStartupFailure(new Error("bad import"));
  });

  it("replaces the splash with the failure and its error", () => {
    expect(root()?.querySelector(".haku-splash")).toBeNull();
    expect(root()?.querySelector("h1")?.textContent).toBe("The interface could not start");
    expect(root()?.querySelector("details pre")?.textContent).toContain("bad import");
  });

  it("keeps a way to drag and close the window", () => {
    expect(root()?.querySelector<HTMLElement>("header")?.dataset.tauriDragRegion).toBe("");

    root()?.querySelector<HTMLButtonElement>('button[aria-label="Close"]')?.click();
    expect(close).toHaveBeenCalledOnce();
  });
});
