import type { CloseConfirmationRequested } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { confirm, listen } = vi.hoisted(() => ({
  confirm: vi.fn(() => Object.assign(Promise.resolve(false), { dismiss: vi.fn() })),
  listen: vi.fn<(callback: (event: { payload: CloseConfirmationRequested }) => void) => Promise<() => void>>(
    async () => () => {},
  ),
}));
vi.mock("@bindings", () => ({ events: { closeConfirmationRequested: () => ({ listen }) } }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ label: "window-1" }) }));
vi.mock("@features/feedback/feedback", () => ({ feedback: () => ({ dialog: { confirm } }) }));
vi.mock("@ipc/commands", () => ({ commands: { confirmClose: vi.fn() } }));

import { useCloseConfirmation } from "./use-close-confirmation";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Asker() {
  useCloseConfirmation();
  return null;
}

describe("useCloseConfirmation", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(async () => {
    confirm.mockClear();
    container = document.createElement("div");
    root = createRoot(container);
    await act(async () => root.render(<Asker />));
  });

  afterEach(async () => {
    await act(async () => root.unmount());
  });

  const ask = async (payload: CloseConfirmationRequested) => {
    await act(async () => listen.mock.calls.at(-1)![0]({ payload }));
    return confirm.mock.calls.at(-1) as unknown as [string, { title: string; confirmLabel: string }];
  };

  it("speaks of quitting when the close quits Haku", async () => {
    const [message, options] = await ask({ tab: null, ending: "quit", downloads: 2 });

    expect(options.title).toBe("Quit Haku?");
    expect(message).toBe("2 downloads are still running. Quitting cancels them.");
    expect(options.confirmLabel).toBe("Quit and cancel");
  });

  it("speaks of closing when the close ends private browsing", async () => {
    const [message, options] = await ask({ tab: 3, ending: "privateSession", downloads: 1 });

    expect(options.title).toBe("Close private browsing?");
    expect(message).toBe("A download is still running. Closing cancels it.");
    expect(options.confirmLabel).toBe("Close and cancel");
  });
});
