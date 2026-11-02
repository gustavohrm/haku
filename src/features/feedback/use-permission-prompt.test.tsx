import type { PageDialog, PermissionRequest, Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { usePermissionPrompt } from "./use-permission-prompt";

const { confirm, dismiss } = vi.hoisted(() => ({ confirm: vi.fn(), dismiss: vi.fn() }));
vi.mock("./feedback", () => ({ feedback: () => ({ dialog: { confirm } }) }));
vi.mock("@ipc/commands", () => ({ commands: { answerPermission: vi.fn() } }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let settle: (result: boolean) => void;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  confirm.mockImplementation(() => {
    const result = new Promise<boolean>((resolve) => (settle = resolve));
    return Object.assign(result, { dismiss });
  });
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.clearAllMocks();
});

const camera: PermissionRequest = { id: 7, kind: "camera", origin: "https://meet.example" };
const tab = (permission: PermissionRequest | null, dialog: PageDialog | null = null) =>
  ({ id: 1, permission, dialog }) as Tab;

function Prompt({ tab, isPrivate }: { tab: Tab | null; isPrivate: boolean }) {
  usePermissionPrompt(tab, isPrivate);
  return null;
}

const show = (shown: Tab | null, isPrivate = false) =>
  act(() => root.render(<Prompt tab={shown} isPrivate={isPrivate} />));

/** The class the hook marks its dialog with, as the toaster would set it. */
const marker = () => (confirm.mock.calls[0]![1] as { className: string }).className;

describe("usePermissionPrompt", () => {
  it("asks under the origin's name, with Allow and Block", () => {
    show(tab(camera));

    expect(confirm).toHaveBeenCalledExactlyOnceWith(
      "You can change this later in Settings, under Site permissions.",
      expect.objectContaining({
        title: "https://meet.example wants to use your camera",
        confirmLabel: "Allow",
        cancelLabel: "Block",
        backdropDismiss: false,
      }),
    );
  });

  it("does not point a private window to settings, which never list its decisions", () => {
    show(tab(camera), true);

    expect(confirm).toHaveBeenCalledExactlyOnceWith(
      "Haku forgets this when you close your last private window.",
      expect.anything(),
    );
  });

  it("sends Allow and Block as they were pressed", async () => {
    show(tab(camera));
    await act(async () => settle(true));
    expect(commands.answerPermission).toHaveBeenLastCalledWith(1, 7, "allow");

    show(tab({ ...camera, id: 8 }));
    await act(async () => settle(false));
    expect(commands.answerPermission).toHaveBeenLastCalledWith(1, 8, "block");
  });

  it("dismisses rather than blocks when its dialog is closed with Escape", async () => {
    show(tab(camera));
    const dialog = document.createElement("dialog");
    dialog.className = `toast-dialog ${marker()}`;
    document.body.append(dialog);

    dialog.dispatchEvent(new Event("cancel"));
    await act(async () => settle(false));

    expect(commands.answerPermission).toHaveBeenCalledExactlyOnceWith(1, 7, "dismiss");
    dialog.remove();
  });

  it("still blocks when Escape closed some other dialog first", async () => {
    show(tab(camera));
    const other = document.createElement("dialog");
    document.body.append(other);

    other.dispatchEvent(new Event("cancel"));
    await act(async () => settle(false));

    expect(commands.answerPermission).toHaveBeenCalledExactlyOnceWith(1, 7, "block");
    other.remove();
  });

  it("waits for the tab's page dialog to be answered first", () => {
    const alert: PageDialog = { id: 1, kind: "alert", message: "Hi", defaultText: "", url: "https://meet.example" };
    show(tab(camera, alert));
    expect(confirm).not.toHaveBeenCalled();

    show(tab(camera));
    expect(confirm).toHaveBeenCalledOnce();
  });

  it("closes without answering when Rust withdraws the request", async () => {
    show(tab(camera));
    show(tab(null));
    await act(async () => settle(false));

    expect(dismiss).toHaveBeenCalledOnce();
    expect(commands.answerPermission).not.toHaveBeenCalled();
  });
});
