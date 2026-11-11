import type { Download } from "@bindings";
import { barHoldStore } from "@features/bars/holds";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { commands, listed } = vi.hoisted(() => ({
  commands: {
    openTab: vi.fn(),
    pauseDownload: vi.fn(),
    cancelDownload: vi.fn(),
    openDownload: vi.fn(),
    showDownload: vi.fn(),
  },
  listed: { downloads: [] as Download[] },
}));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({ useDownloads: () => listed.downloads }));

import { DownloadsButton } from "./downloads-button";
import { download } from "./test-download";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("DownloadsButton", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    for (const command of Object.values(commands)) {
      command.mockClear();
    }
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
  });

  const button = () => container.querySelector<HTMLButtonElement>('button[aria-label="Downloads"]');
  const list = () => document.querySelector('section[aria-label="Downloads"]');

  it("is absent until a download starts this session", async () => {
    listed.downloads = [download({ recent: false, state: "completed" })];
    await act(async () => root.render(<DownloadsButton />));
    expect(button()).toBeNull();
  });

  it("lists this session's downloads with what can be done to each", async () => {
    listed.downloads = [
      download({ id: 2 }),
      download({ id: 1, recent: false, state: "completed", path: String.raw`C:\old.zip` }),
    ];
    await act(async () => root.render(<DownloadsButton />));
    await act(async () => button()?.click());

    expect(list()?.textContent).toContain("report (1).pdf");
    expect(list()?.textContent).toContain("512 kB of 1 MB");
    expect(list()?.textContent).not.toContain("old.zip");
    expect(list()?.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("50");

    await act(async () => list()?.querySelector<HTMLButtonElement>('[aria-label="Pause: report (1).pdf"]')?.click());
    expect(commands.pauseDownload).toHaveBeenCalledWith(2);
  });

  it("holds a floating toolbar while the list is open", async () => {
    listed.downloads = [download()];
    await act(async () => root.render(<DownloadsButton />));

    await act(async () => button()?.click());
    expect(barHoldStore.get().toolbar).toBe(1);
    await act(async () => button()?.click());
    expect(barHoldStore.get().toolbar).toBe(0);
  });

  it("opens a completed file from its name, or its folder, through Rust", async () => {
    listed.downloads = [download({ state: "completed" })];
    await act(async () => root.render(<DownloadsButton />));
    await act(async () => button()?.click());

    const name = list()?.querySelector<HTMLButtonElement>('[aria-label="Open file: report (1).pdf"]');
    expect(name?.textContent).toBe("report (1).pdf");
    await act(async () => name?.click());
    await act(async () =>
      list()?.querySelector<HTMLButtonElement>('[aria-label="Show in folder: report (1).pdf"]')?.click(),
    );
    expect(commands.openDownload).toHaveBeenCalledWith(1);
    expect(commands.showDownload).toHaveBeenCalledWith(1);
  });

  it("shows Show in folder on the row's hover or focus", async () => {
    listed.downloads = [download({ state: "completed" })];
    await act(async () => root.render(<DownloadsButton />));
    await act(async () => button()?.click());

    const show = list()?.querySelector('[aria-label="Show in folder: report (1).pdf"]');
    expect(show?.classList).toContain("opacity-0");
    expect(show?.classList).toContain("group-hover:opacity-100");
    expect(show?.classList).toContain("group-focus-within:opacity-100");
  });

  it("keeps a running download's Pause and Cancel in view", async () => {
    listed.downloads = [download()];
    await act(async () => root.render(<DownloadsButton />));
    await act(async () => button()?.click());

    for (const label of ["Pause: report (1).pdf", "Cancel: report (1).pdf"]) {
      expect(list()?.querySelector(`[aria-label="${label}"]`)?.classList).not.toContain("opacity-0");
    }
  });

  it("shows a failure in the download's own entry", async () => {
    listed.downloads = [download({ state: "failed", failure: "server" })];
    await act(async () => root.render(<DownloadsButton />));
    await act(async () => button()?.click());

    expect(list()?.textContent).toContain("Failed — the server stopped it");
    expect(list()?.querySelector('[aria-label="Retry: report (1).pdf"]')).toBeNull();
  });

  it("ends with Manage downloads after a separator, which opens the downloads page", async () => {
    listed.downloads = [download()];
    await act(async () => root.render(<DownloadsButton />));
    await act(async () => button()?.click());

    const manage = list()?.lastElementChild;
    expect(manage?.textContent).toBe("Manage downloads");
    expect(manage?.previousElementSibling?.tagName).toBe("HR");
    await act(async () => (manage as HTMLButtonElement).click());
    expect(commands.openTab).toHaveBeenCalledWith("haku://downloads", true, "other");
    expect(list()).toBeNull();
  });
});
