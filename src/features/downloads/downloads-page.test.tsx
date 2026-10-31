import type { Download } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { commands, listed } = vi.hoisted(() => ({
  commands: { clearDownloads: vi.fn(), resumeDownload: vi.fn(), cancelDownload: vi.fn() },
  listed: { downloads: [] as Download[] },
}));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({ useDownloads: () => listed.downloads }));

import { DownloadsPage } from "./downloads-page";
import { download } from "./test-download";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("DownloadsPage", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
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
  });

  const clear = () =>
    [...container.querySelectorAll("button")].find((button) => button.textContent === "Clear finished");

  it("says when there is nothing to list", async () => {
    listed.downloads = [];
    await act(async () => root.render(<DownloadsPage />));
    expect(container.textContent).toContain("No downloads yet.");
    expect(clear()?.disabled).toBe(true);
  });

  it("lists earlier sessions' downloads too", async () => {
    listed.downloads = [download({ recent: false, state: "completed" })];
    await act(async () => root.render(<DownloadsPage />));
    expect(container.textContent).toContain("report (1).pdf");
  });

  it("retries a failure the engine can resume", async () => {
    listed.downloads = [download({ state: "failed", failure: "network", canResume: true })];
    await act(async () => root.render(<DownloadsPage />));
    await act(async () => container.querySelector<HTMLButtonElement>('[aria-label="Retry: report (1).pdf"]')?.click());
    expect(commands.resumeDownload).toHaveBeenCalledWith(1);
  });

  it("clears finished downloads only once one has ended", async () => {
    listed.downloads = [download()];
    await act(async () => root.render(<DownloadsPage />));
    expect(clear()?.disabled).toBe(true);

    listed.downloads = [download(), download({ id: 2, state: "cancelled" })];
    await act(async () => root.render(<DownloadsPage />));
    await act(async () => clear()?.click());
    expect(commands.clearDownloads).toHaveBeenCalledOnce();
  });
});
