import { type HistoryFilter, type HistoryVisit } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { commands, state, updateSettings, openMenu } = vi.hoisted(() => {
  const ok = (data: unknown = null) => Promise.resolve({ status: "ok", data });
  return {
    commands: {
      queryHistory: vi.fn(),
      historySites: vi.fn(() => ok([])),
      countHistory: vi.fn(() => ok(0)),
      deleteHistory: vi.fn(() => ok(0)),
      deleteHistoryVisits: vi.fn(() => ok(0)),
      setHistoryPaused: vi.fn(() => ok()),
      navigateTab: vi.fn(() => ok()),
      openTab: vi.fn(() => ok()),
    },
    state: { isPrivate: false, paused: false, enabled: true },
    updateSettings: vi.fn(),
    openMenu: vi.fn(),
  };
});
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({
  useBrowserState: () => ({ private: state.isPrivate, historyPaused: state.paused }),
  useSettings: () => ({ historyEnabled: state.enabled, historyRules: [] }),
}));
vi.mock("@ipc/store", () => ({ updateSettings, tryUpdateSettings: vi.fn() }));
vi.mock("@features/context-menu/store", () => ({ openMenu }));
vi.mock("@features/feedback/feedback", () => ({ feedback: vi.fn(), reportUnexpected: vi.fn() }));

import { HistoryPage, SEARCH_DELAY_MS } from "./history-page";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const NOW = Date.now();
const visit = (id: number, url: string, visitedAt = NOW, title = `Page ${id}`): HistoryVisit => ({
  id,
  url,
  host: new URL(url).host,
  title,
  visitedAt,
  transition: "link",
});
const page = (visits: HistoryVisit[]) => ({ status: "ok", data: { visits, next: null } });
const lastFilter = () => commands.queryHistory.mock.calls.at(-1)![0] as HistoryFilter;

describe("HistoryPage", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.clearAllMocks();
    state.isPrivate = false;
    state.paused = false;
    state.enabled = true;
    commands.queryHistory.mockResolvedValue(
      page([visit(1, "https://a.com/x"), visit(2, "https://b.com/"), visit(3, "https://a.com/x", NOW - 1000)]),
    );
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.useRealTimers();
  });

  const render = (url = "haku://history") => act(async () => root.render(<HistoryPage tabId={7} url={url} />));
  const button = (text: string) =>
    [...container.querySelectorAll("button")].find((each) => each.textContent?.trim() === text);
  const click = (element: Element | undefined) =>
    act(async () => element!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
  const type = async (input: HTMLInputElement, value: string) => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      set.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  };

  it("groups visits under Today, folding repeats of a page into one row", async () => {
    await render();
    expect(container.querySelector("h2")?.textContent).toBe("Today");
    const rows = container.querySelectorAll("section > ul > li");
    expect(rows).toHaveLength(2);
    expect(rows[0]!.textContent).toContain("2 visits");
  });

  it("opens a row in this tab, and in a new tab with Ctrl", async () => {
    await render();
    const title = [...container.querySelectorAll("button")].find((each) => each.title === "https://b.com/")!;
    await click(title);
    expect(commands.navigateTab).toHaveBeenCalledWith(7, "https://b.com/", "other");
    await act(async () => title.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true })));
    expect(commands.openTab).toHaveBeenCalledWith("https://b.com/", false, "other");
  });

  it("starts filtered to the site its address names", async () => {
    await render("haku://history?site=a.com");
    expect(lastFilter().include).toEqual([{ kind: "site", value: "a.com" }]);
    expect(container.querySelector("[aria-label='Active filters']")?.textContent).toContain("a.com");
    expect(button("Delete all matching…")).toBeDefined();
  });

  it("searches once typing rests", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    await render();
    const calls = commands.queryHistory.mock.calls.length;
    await type(container.querySelector("input[type=search]")!, "rust");
    expect(commands.queryHistory.mock.calls.length).toBe(calls);
    await act(async () => vi.advanceTimersByTime(SEARCH_DELAY_MS));
    expect(lastFilter().text).toBe("rust");
  });

  it("offers Delete all matching only with a filter", async () => {
    await render();
    expect(button("Delete all matching…")).toBeUndefined();
  });

  it("deletes the selected visits, every visit of a folded row included", async () => {
    await render();
    const box = container.querySelector<HTMLInputElement>("input[type=checkbox][aria-label='Select Page 1']")!;
    await click(box);
    await click(button("Delete 2 selected visits"));
    expect(commands.deleteHistoryVisits).toHaveBeenCalledWith([1, 3]);
  });

  it("shows why a pattern is invalid in place of the list", async () => {
    commands.queryHistory.mockResolvedValue({
      status: "error",
      error: { kind: "InvalidPattern", message: "Unclosed group" },
    });
    await render();
    expect(container.querySelector("[role=alert]")?.textContent).toBe("Unclosed group");
  });

  it("lists sites, and a site opens its visits", async () => {
    commands.historySites.mockResolvedValue({
      status: "ok",
      data: [{ host: "a.com", visits: 2, lastVisitedAt: NOW }],
    });
    await render();
    await click(button("Sites"));
    const site = [...container.querySelectorAll("li > button")].find(
      (each) => each.querySelector(".font-medium")?.textContent === "a.com",
    );
    await click(site);
    expect(button("Visits")?.getAttribute("aria-pressed")).toBe("true");
    expect(lastFilter().include).toEqual([{ kind: "site", value: "a.com" }]);
  });

  it("says when recording is paused, and resumes it", async () => {
    state.paused = true;
    await render();
    expect(container.querySelector("output")?.textContent).toContain("paused");
    await click(container.querySelector("output button")!);
    expect(commands.setHistoryPaused).toHaveBeenCalledWith(false);
  });

  it("links to the history settings", async () => {
    await render();
    await click(container.querySelector("[aria-label='History settings']")!);
    expect(commands.navigateTab).toHaveBeenCalledWith(7, "haku://settings/browsing#historyEnabled", "other");
  });

  it("turns recording back on from the banner when it is off", async () => {
    state.enabled = false;
    await render();
    await click(button("Turn on"));
    expect(updateSettings).toHaveBeenCalled();
  });
});
