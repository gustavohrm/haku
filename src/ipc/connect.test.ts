import { type BrowserState, type Download, type HakuError, type Settings } from "@bindings";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, reportFailure } from "./commands";
import {
  browserStore,
  connect,
  downloadsStore,
  EMPTY_STATE,
  extensionsStore,
  reloadSettings,
  settingsStore,
} from "./store";

const listen = vi.hoisted(() => vi.fn(async () => () => {}));
const stateListen = vi.hoisted(() =>
  vi.fn<(callback: (event: { payload: BrowserState }) => void) => Promise<() => void>>(async () => () => {}),
);
const failureListen = vi.hoisted(() =>
  vi.fn<(callback: (event: { payload: HakuError }) => void) => Promise<() => void>>(async () => () => {}),
);
const downloadsListen = vi.hoisted(() =>
  vi.fn<(callback: (event: { payload: Download[] }) => void) => Promise<() => void>>(async () => () => {}),
);
const failureTarget = vi.hoisted(() => vi.fn());
vi.mock("@bindings", () => ({
  events: {
    stateChanged: () => ({ listen: stateListen }),
    failureReported: (target: unknown) => {
      failureTarget(target);
      return { listen: failureListen };
    },
    settingsChanged: { listen },
    extensionsChanged: { listen },
    downloadsChanged: () => ({ listen: downloadsListen }),
  },
}));
const CURRENT_WEBVIEW = vi.hoisted(() => ({ label: "window-1" }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => CURRENT_WEBVIEW }));
vi.mock("./commands", () => ({
  commands: { getState: vi.fn(), getSettings: vi.fn(), extensions: vi.fn(), downloads: vi.fn() },
  reportFailure: vi.fn(),
}));

const STATE = { ...EMPTY_STATE, capacity: 3 };
const SETTINGS = { theme: "dark" } as Settings;
const failure = { status: "error", error: { kind: "Storage", message: "locked" } } as const;

describe("connect", () => {
  beforeEach(() => {
    browserStore.set(EMPTY_STATE);
    settingsStore.set(null);
    extensionsStore.set(null);
    downloadsStore.set([]);
    vi.mocked(commands.getState).mockResolvedValue({ status: "ok", data: STATE });
    vi.mocked(commands.getSettings).mockResolvedValue(failure);
    vi.mocked(commands.extensions).mockResolvedValue({ status: "ok", data: { installed: [] } } as never);
    vi.mocked(commands.downloads).mockResolvedValue({ status: "ok", data: [] });
  });

  it("keeps what loaded when another part fails to", async () => {
    await expect(connect()).rejects.toThrow("locked");

    expect(browserStore.get()).toBe(STATE);
    expect(extensionsStore.get()).toEqual({ installed: [] });
    expect(settingsStore.get()).toBeNull();
  });

  it("keeps the downloads that loaded when another part fails to", async () => {
    const listed = [{ id: 1 }] as Download[];
    vi.mocked(commands.downloads).mockResolvedValueOnce({ status: "ok", data: listed });

    await expect(connect()).rejects.toThrow("locked");

    expect(downloadsStore.get()).toBe(listed);
  });

  it("keeps a downloads event over a delayed startup list", async () => {
    const newer = [{ id: 2 }] as Download[];
    vi.mocked(commands.getSettings).mockResolvedValueOnce({ status: "ok", data: SETTINGS });
    vi.mocked(commands.downloads).mockImplementationOnce(async () => {
      downloadsListen.mock.calls.at(-1)![0]({ payload: newer });
      return { status: "ok", data: [] };
    });

    const stop = await connect();
    expect(downloadsStore.get()).toBe(newer);
    stop();
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

  it("listens for this window's failures before asking for its state", async () => {
    failureListen.mockClear();
    let isListeningWhenAsked = false;
    vi.mocked(commands.getState).mockImplementationOnce(async () => {
      isListeningWhenAsked = failureListen.mock.calls.length > 0;
      return { status: "ok", data: STATE };
    });

    await connect().catch(() => {});

    expect(isListeningWhenAsked).toBe(true);
    expect(failureTarget).toHaveBeenLastCalledWith(CURRENT_WEBVIEW);
  });

  it("reports a failure Rust sends as any other failure is reported", async () => {
    await connect().catch(() => {});
    const error = { kind: "Storage", message: "history could not be opened" } as const;

    failureListen.mock.calls.at(-1)![0]({ payload: error });

    expect(reportFailure).toHaveBeenCalledWith(error);
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
