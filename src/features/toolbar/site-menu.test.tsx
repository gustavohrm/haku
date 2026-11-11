import { type Tab } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { commands, settings, updateSettings, tryUpdateSettings } = vi.hoisted(() => {
  const ok = (data: unknown = null) => Promise.resolve({ status: "ok", data });
  return {
    commands: {
      setTabFixed: vi.fn(() => ok()),
      openTab: vi.fn(() => ok()),
      historyRuleFor: vi.fn(() => ok(null)),
      countHistory: vi.fn(() => ok(0)),
      deleteHistory: vi.fn(() => ok(0)),
    },
    settings: { keptSites: [] as string[], historyEnabled: true, historyRules: [] },
    updateSettings: vi.fn(),
    tryUpdateSettings: vi.fn(() => Promise.resolve({ status: "ok", data: {} })),
  };
});
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({
  useBrowserState: () => ({ private: false, historyPaused: false }),
  useSettings: () => settings,
}));
vi.mock("@ipc/store", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  updateSettings,
  tryUpdateSettings,
}));
vi.mock("@features/feedback/feedback", () => ({
  feedback: () => ({ success: vi.fn(), info: vi.fn() }),
  reportUnexpected: vi.fn(),
}));

import { SiteMenu } from "./site-menu";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const tab = (fixed = false): Tab => ({
  id: 7,
  history: { entries: [{ url: "https://example.com/", title: "Example", favicon: null }], index: 0 },
  presence: { status: "live", slot: 1 },
  scroll: { x: 0, y: 0 },
  fixed,
  activeAt: 0,
  dialog: null,
  permission: null,
  audible: false,
  zoom: 100,
  relieved: false,
  restoring: false,
});

describe("SiteMenu", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.clearAllMocks();
    settings.keptSites = [];
    commands.countHistory.mockResolvedValue({ status: "ok", data: 0 });
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
  });

  const open = async (fixed = false) => {
    await act(async () => root.render(<SiteMenu tab={tab(fixed)} url="https://example.com/" warnInsecure={false} />));
    await act(async () => container.querySelector<HTMLButtonElement>("[aria-label='Site information']")!.click());
  };
  const menu = () => document.body.querySelector("section[aria-label='Site information']")!;
  const button = (text: string) =>
    [...menu().querySelectorAll("button")].find((each) => each.textContent?.trim() === text);
  const toggle = (name: string) =>
    [...menu().querySelectorAll<HTMLInputElement>("input.switch")].find(
      (each) => document.getElementById(each.getAttribute("aria-labelledby")!)?.textContent === name,
    )!;
  const click = (element: Element | undefined) => act(async () => (element as HTMLElement).click());

  it("says under each Keep loaded switch what it reaches", async () => {
    await open();
    expect(menu().textContent).toContain("This tab only");
    expect(menu().textContent).toContain("Every tab on this site");
  });

  it("warns before keeping the tab loaded, and keeps it only once that is confirmed", async () => {
    await open();
    await click(toggle("Keep this tab loaded"));
    expect(menu().querySelector("[role=alert]")?.textContent).toContain("stays in memory");
    expect(commands.setTabFixed).not.toHaveBeenCalled();
    await click(button("Keep loaded"));
    expect(commands.setTabFixed).toHaveBeenCalledExactlyOnceWith(7, true);
    expect(menu().querySelector("[role=alert]")).toBeNull();
  });

  it("keeps nothing when the warning is cancelled", async () => {
    await open();
    await click(toggle("Keep this tab loaded"));
    await click(button("Cancel"));
    expect(commands.setTabFixed).not.toHaveBeenCalled();
    expect(toggle("Keep this tab loaded").checked).toBe(false);
  });

  it("stops keeping the tab loaded without asking", async () => {
    await open(true);
    await click(toggle("Keep this tab loaded"));
    expect(menu().querySelector("[role=alert]")).toBeNull();
    expect(commands.setTabFixed).toHaveBeenCalledExactlyOnceWith(7, false);
  });

  it("warns the same way before keeping the site loaded", async () => {
    await open();
    await click(toggle("Keep this site loaded"));
    expect(menu().querySelector("[role=alert]")?.textContent).toContain("Tabs on this site stay in memory");
    expect(updateSettings).not.toHaveBeenCalled();
    await click(button("Keep loaded"));
    expect(updateSettings).toHaveBeenCalledOnce();
  });

  it("asks in place, in the settings form's words, whether to delete what is recorded before never recording the site", async () => {
    commands.countHistory.mockResolvedValue({ status: "ok", data: 3 });
    await open();
    await vi.waitFor(() => expect(button("Never record this site…")).toBeDefined());
    await click(button("Never record this site…"));
    expect(menu().isConnected).toBe(true);
    expect(menu().querySelector("[role=alert]")?.textContent).toContain("3 recorded visits match this rule.");
    expect(tryUpdateSettings).not.toHaveBeenCalled();
    await click(button("Never record and delete 3 visits"));
    await vi.waitFor(() => expect(commands.deleteHistory).toHaveBeenCalled());
    expect(tryUpdateSettings).toHaveBeenCalledOnce();
  });

  it("never records the site at once, with the menu still open, when nothing is recorded", async () => {
    await open();
    await vi.waitFor(() => expect(button("Never record this site…")).toBeDefined());
    await click(button("Never record this site…"));
    await vi.waitFor(() => expect(tryUpdateSettings).toHaveBeenCalledOnce());
    expect(menu().isConnected).toBe(true);
    expect(commands.deleteHistory).not.toHaveBeenCalled();
  });
});
