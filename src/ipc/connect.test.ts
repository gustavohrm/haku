import { type BrowserState, type Settings } from "@bindings";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "./commands";
import { browserStore, connect, EMPTY_STATE, extensionsStore, reloadSettings, settingsStore } from "./store";

const listen = vi.hoisted(() => vi.fn(async () => () => {}));
const stateListen = vi.hoisted(() =>
  vi.fn<(callback: (event: { payload: BrowserState }) => void) => Promise<() => void>>(async () => () => {}),
);
vi.mock("@bindings", () => ({
  events: {
    stateChanged: () => ({ listen: stateListen }),
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

  it.each([1, 2])("keeps a state event at revision %s over a delayed startup snapshot", async (layoutRevision) => {
    const newer = { ...STATE, capacity: 4, layoutRevision };
    vi.mocked(commands.getSettings).mockResolvedValueOnce({ status: "ok", data: SETTINGS });
    vi.mocked(commands.getState).mockImplementationOnce(async () => {
      stateListen.mock.calls.at(-1)![0]({ payload: newer });
      return { status: "ok", data: { ...STATE, layoutRevision: 1 } };
    });

    const stop = await connect();
    expect(browserStore.get()).toBe(newer);
    stop();
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
