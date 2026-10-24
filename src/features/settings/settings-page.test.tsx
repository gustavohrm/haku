import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SettingsPage } from "./settings-page";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const reloadSettings = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("@ipc/hooks", () => ({ useSettings: () => null }));
vi.mock("@ipc/store", async (original) => ({ ...(await original<object>()), reloadSettings }));
vi.mock("@ipc/commands", () => ({ commands: {} }));

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
});

describe("SettingsPage", () => {
  it("offers to load the settings again when they could not be loaded", async () => {
    act(() => root.render(<SettingsPage tabId={1} url="haku://settings" />));

    expect(container.querySelector("h1")?.textContent).toBe("Settings could not be loaded");
    await act(async () => container.querySelector("button")?.click());
    expect(reloadSettings).toHaveBeenCalledOnce();
  });
});
