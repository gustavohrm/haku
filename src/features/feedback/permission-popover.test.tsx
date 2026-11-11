import type { PageDialog, PermissionRequest, Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { PermissionPopover } from "./permission-popover";

vi.mock("@ipc/commands", () => ({
  commands: { answerPermission: vi.fn(() => Promise.resolve({ status: "ok", data: null })) },
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom lays nothing out, so there is nothing for the prompt to follow.
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    disconnect() {}
  },
);

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.clearAllMocks();
});

const camera: PermissionRequest = { id: 7, kind: "camera", origin: "https://meet.example" };
const tab = (permission: PermissionRequest | null, dialog: PageDialog | null = null) =>
  ({ id: 1, permission, dialog }) as Tab;

const show = (shown: Tab | null, isPrivate = false) =>
  act(() => root.render(<PermissionPopover tab={shown} isPrivate={isPrivate} />));

const prompt = () => document.querySelector<HTMLElement>('[role="alertdialog"]');
const press = (name: string) => {
  const button = [...(prompt()?.querySelectorAll("button") ?? [])].find(
    (candidate) => candidate.textContent === name || candidate.getAttribute("aria-label") === name,
  );
  act(() => button?.click());
};

describe("PermissionPopover", () => {
  it("asks under the origin's name, with Allow and Block", () => {
    show(tab(camera));

    expect(prompt()?.textContent).toContain("https://meet.example wants to use your camera");
    expect(prompt()?.textContent).toContain("You can change this later in Settings, under Site permissions.");
    expect([...prompt()!.querySelectorAll("button")].map((button) => button.textContent)).toEqual([
      "",
      "Block",
      "Allow",
    ]);
  });

  it("does not point a private window to settings, which never list its decisions", () => {
    show(tab(camera), true);
    expect(prompt()?.textContent).toContain("Haku forgets this when you close your last private window.");
  });

  it("leaves the window usable: not modal, and takes no focus from the page", () => {
    const page = document.createElement("button");
    document.body.append(page);
    page.focus();

    show(tab(camera));

    expect(prompt()?.getAttribute("aria-modal")).toBe("false");
    expect(document.activeElement).toBe(page);
    page.remove();
  });

  it("stays open on a click elsewhere", () => {
    show(tab(camera));
    act(() => document.body.dispatchEvent(new Event("pointerdown", { bubbles: true })));
    expect(prompt()).not.toBeNull();
    expect(commands.answerPermission).not.toHaveBeenCalled();
  });

  it("sends Allow and Block as they were pressed, and goes once answered", () => {
    show(tab(camera));
    press("Allow");
    expect(commands.answerPermission).toHaveBeenLastCalledWith(1, 7, "allow");
    expect(prompt()).toBeNull();

    show(tab({ ...camera, id: 8 }));
    press("Block");
    expect(commands.answerPermission).toHaveBeenLastCalledWith(1, 8, "block");
  });

  it("shows the request again when its answer did not reach Rust", async () => {
    vi.mocked(commands.answerPermission).mockResolvedValueOnce({
      status: "error",
      error: { kind: "Internal", message: "" },
    } as never);
    show(tab(camera));
    await act(async () => press("Allow"));
    expect(prompt()).not.toBeNull();

    await act(async () => press("Allow"));
    expect(prompt()).toBeNull();
  });

  it("dismisses rather than blocks when closed, or on Escape inside it", () => {
    show(tab(camera));
    press("Not now");
    expect(commands.answerPermission).toHaveBeenLastCalledWith(1, 7, "dismiss");

    show(tab({ ...camera, id: 8 }));
    const block = prompt()!.querySelector("button")!;
    act(() => block.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(commands.answerPermission).toHaveBeenLastCalledWith(1, 8, "dismiss");
  });

  it("waits for the tab's page dialog to be answered first", () => {
    const alert: PageDialog = { id: 1, kind: "alert", message: "Hi", defaultText: "", url: "https://meet.example" };
    show(tab(camera, alert));
    expect(prompt()).toBeNull();

    show(tab(camera));
    expect(prompt()).not.toBeNull();
  });

  it("goes without answering when Rust withdraws the request", () => {
    show(tab(camera));
    show(tab(null));

    expect(prompt()).toBeNull();
    expect(commands.answerPermission).not.toHaveBeenCalled();
  });
});
