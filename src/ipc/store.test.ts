import { type Settings } from "@bindings";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "./commands";
import { applySettingsPreset, createStore, settingsStore, updateSettings } from "./store";

vi.mock("./commands", () => ({
  commands: { setSettings: vi.fn(), applyPreset: vi.fn() },
}));

const setSettings = vi.mocked(commands.setSettings);
const applyPreset = vi.mocked(commands.applyPreset);

const SETTINGS: Settings = {
  webviewCapacity: 1,
  keptMemoryMb: 0,
  freezeTabs: "smart",
  discardTabs: "never",
  keptSites: [],
  theme: "system",
  tabLayout: "horizontal",
  barLayout: "docked",
  sidebarSide: "left",
  toolbarPlacement: "top",
  sidebarWidth: 240,
  locale: "en",
  searchUrl: "https://example.com/?q=%s",
  homeUrl: "haku://new-tab",
  disabledExtensions: [],
  pinnedExtensions: [],
};

describe("createStore", () => {
  it("returns the initial value before anything is set", () => {
    expect(createStore(7).get()).toBe(7);
  });

  it("notifies subscribers when the value changes", () => {
    const store = createStore(0);
    const listener = vi.fn();
    store.subscribe(listener);

    store.set(1);

    expect(listener).toHaveBeenCalledTimes(1);
    expect(store.get()).toBe(1);
  });

  it("stays quiet when set to the value it already holds", () => {
    const store = createStore("a");
    const listener = vi.fn();
    store.subscribe(listener);

    store.set("a");

    expect(listener).not.toHaveBeenCalled();
  });

  it("stops notifying once a subscriber unsubscribes", () => {
    const store = createStore(0);
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    unsubscribe();
    store.set(1);

    expect(listener).not.toHaveBeenCalled();
  });

  it("notifies every subscriber", () => {
    const store = createStore(0);
    const first = vi.fn();
    const second = vi.fn();
    store.subscribe(first);
    store.subscribe(second);

    store.set(1);

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("treats a new object as a change even when its contents match", () => {
    const store = createStore({ value: 1 });
    const listener = vi.fn();
    store.subscribe(listener);

    store.set({ value: 1 });

    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe("updateSettings", () => {
  beforeEach(() => {
    setSettings.mockReset();
    setSettings.mockImplementation(async (settings) => ({ status: "ok", data: settings }));
    settingsStore.set(SETTINGS);
  });

  it("stores what Rust returned for the change", async () => {
    const stored = await updateSettings((current) => ({ ...current, theme: "dark" }));

    expect(stored?.theme).toBe("dark");
    expect(settingsStore.get()?.theme).toBe("dark");
  });

  it("applies two changes issued back to back, the second building on the first", async () => {
    const first = updateSettings((current) => ({ ...current, disabledExtensions: ["a"] }));
    const second = updateSettings((current) => ({
      ...current,
      disabledExtensions: [...current.disabledExtensions, "b"],
    }));

    await Promise.all([first, second]);

    expect(setSettings).toHaveBeenLastCalledWith(expect.objectContaining({ disabledExtensions: ["a", "b"] }));
    expect(settingsStore.get()?.disabledExtensions).toEqual(["a", "b"]);
  });

  it("resolves null for a failed change and still runs the next one", async () => {
    const error = { kind: "Storage", message: "disk full" } as const;
    setSettings.mockResolvedValueOnce({ status: "error", error });

    const failed = await updateSettings((current) => ({ ...current, theme: "dark" }));
    const next = await updateSettings((current) => ({ ...current, theme: "light" }));

    expect(failed).toBeNull();
    expect(next?.theme).toBe("light");
    expect(settingsStore.get()?.theme).toBe("light");
  });

  it("leaves the stored settings alone when a change fails", async () => {
    const error = { kind: "Storage", message: "disk full" } as const;
    setSettings.mockResolvedValueOnce({ status: "error", error });

    await updateSettings((current) => ({ ...current, theme: "dark" }));

    expect(settingsStore.get()).toBe(SETTINGS);
  });

  it("resolves null without sending anything when settings are not loaded", async () => {
    settingsStore.set(null);

    const result = await updateSettings((current) => ({ ...current, theme: "dark" }));

    expect(result).toBeNull();
    expect(setSettings).not.toHaveBeenCalled();
  });

  it("still runs the next change after a change that throws", async () => {
    const thrown = updateSettings(() => {
      throw new Error("bad change");
    });
    const next = updateSettings((current) => ({ ...current, theme: "light" }));

    await expect(thrown).rejects.toThrow("bad change");
    expect((await next)?.theme).toBe("light");
    expect(settingsStore.get()?.theme).toBe("light");
  });

  it("builds a change made while a preset applies on the preset's result", async () => {
    applyPreset.mockResolvedValueOnce({ status: "ok", data: { ...SETTINGS, webviewCapacity: 4 } });

    const preset = applySettingsPreset("balanced");
    const change = updateSettings((current) => ({ ...current, theme: "dark" }));
    await Promise.all([preset, change]);

    expect(setSettings).toHaveBeenLastCalledWith(expect.objectContaining({ webviewCapacity: 4, theme: "dark" }));
    expect(settingsStore.get()).toMatchObject({ webviewCapacity: 4, theme: "dark" });
  });
});
