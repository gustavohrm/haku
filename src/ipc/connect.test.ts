import { type Settings } from "@bindings";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "./commands";
import { browserStore, connect, EMPTY_STATE, extensionsStore, reloadSettings, settingsStore } from "./store";

const listen = vi.hoisted(() => vi.fn(async () => () => {}));
vi.mock("@bindings", () => ({
  events: {
    stateChanged: () => ({ listen }),
    settingsChanged: { listen },
    extensionsChanged: { listen },
  },
}));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({}) }));
vi.mock("./commands", () => ({
  commands: { getState: vi.fn(), getSettings: vi.fn(), extensions: vi.fn() },
}));

const STATE = { ...EMPTY_STATE, capacity: 3 };
const SETTINGS = { theme: "dark" } as Settings;
const failure = { status: "error", error: { kind: "Storage", message: "locked" } } as const;

describe("connect", () => {
  beforeEach(() => {
    browserStore.set(EMPTY_STATE);
    settingsStore.set(null);
    extensionsStore.set(null);
    vi.mocked(commands.getState).mockResolvedValue({ status: "ok", data: STATE });
    vi.mocked(commands.getSettings).mockResolvedValue(failure);
    vi.mocked(commands.extensions).mockResolvedValue({ status: "ok", data: { installed: [] } } as never);
  });

  it("keeps what loaded when another part fails to", async () => {
    await expect(connect()).rejects.toThrow("locked");

    expect(browserStore.get()).toBe(STATE);
    expect(extensionsStore.get()).toEqual({ installed: [] });
    expect(settingsStore.get()).toBeNull();
  });

  it("keeps settings that arrived while they were being read again", async () => {
    const newer = { theme: "light" } as Settings;
    vi.mocked(commands.getSettings).mockImplementationOnce(async () => {
      settingsStore.set(newer);
      return { status: "ok", data: SETTINGS };
    });

    await reloadSettings();

    expect(settingsStore.get()).toBe(newer);
  });

  it("loads the settings again on request", async () => {
    vi.mocked(commands.getSettings).mockResolvedValueOnce({ status: "ok", data: SETTINGS });

    await reloadSettings();

    expect(settingsStore.get()).toBe(SETTINGS);
  });
});
