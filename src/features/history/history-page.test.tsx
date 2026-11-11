import { type HistoryFilter, type HistoryVisit } from "@bindings";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { commands, state, updateSettings, openMenu, info } = vi.hoisted(() => {
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
      historyRuleFor: vi.fn(() => ok(null)),
    },
    state: { isPrivate: false, paused: false, enabled: true },
    updateSettings: vi.fn(),
    openMenu: vi.fn(),
    info: vi.fn(),
  };
});
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/hooks", () => ({
  useBrowserState: () => ({ private: state.isPrivate, historyPaused: state.paused }),
  useSettings: () => ({ historyEnabled: state.enabled, historyRules: [] }),
}));
vi.mock("@ipc/store", () => ({ updateSettings, tryUpdateSettings: vi.fn() }));
vi.mock("@features/context-menu/store", () => ({ openMenu }));
vi.mock("@features/feedback/feedback", () => ({ feedback: () => ({ info }), reportUnexpected: vi.fn() }));

import { type MenuItem } from "@features/overlays/menu-model";

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

  it("stops loading when the first page fails, which the command wrapper reports", async () => {
    commands.queryHistory.mockResolvedValue({ status: "error", error: { kind: "Storage", message: "locked" } });
    await render();
    expect(container.textContent).toContain("Nothing here yet.");
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

  it("says when recording is paused, and resumes it from the banner alone", async () => {
    await render();
    expect(button("Pause until restart")).toBeDefined();
    state.paused = true;
    await render();
    expect(container.querySelector("output")?.textContent).toContain("paused");
    expect(button("Pause until restart")).toBeUndefined();
    expect([...container.querySelectorAll("button")].filter((each) => each.textContent === "Resume")).toHaveLength(1);
    await click(container.querySelector("output button")!);
    expect(commands.setHistoryPaused).toHaveBeenCalledWith(false);
  });

  it("shows a row's checkbox and actions on hover or focus only, until a row is selected", async () => {
    await render();
    const wrapper = (label: string) => container.querySelector(`input[aria-label="Select ${label}"]`)!.parentElement!;
    expect(wrapper("Page 1").className).toContain("opacity-0");
    expect(container.querySelector("[aria-label='Actions for Page 1']")!.className).toContain("opacity-0");
    await click(container.querySelector("input[aria-label='Select Page 1']")!);
    expect(wrapper("Page 2").className).not.toContain("opacity-0");
  });

  it("asks in place before deleting all matching, saying how many visits go", async () => {
    commands.countHistory.mockResolvedValue({ status: "ok", data: 5 });
    await render("haku://history?site=a.com");
    await click(button("Delete all matching…"));
    expect(container.querySelector("[role=alert]")?.textContent).toContain(
      "5 visits will be deleted. This can't be undone.",
    );
    expect(document.activeElement).toBe(button("Cancel"));
    await click(button("Cancel"));
    expect(commands.deleteHistory).not.toHaveBeenCalled();
    expect(button("Delete all matching…")).toBeDefined();

    await click(button("Delete all matching…"));
    await click(button("Delete 5 visits"));
    expect(commands.deleteHistory).toHaveBeenCalledWith(lastFilter());
    expect(button("Delete 5 visits")).toBeUndefined();
  });

  it("says there is nothing to delete rather than asking, when nothing matches", async () => {
    commands.countHistory.mockResolvedValue({ status: "ok", data: 0 });
    await render("haku://history?site=a.com");
    await click(button("Delete all matching…"));
    expect(info).toHaveBeenCalled();
    expect(button("Cancel")).toBeUndefined();
  });

  it("opens Clear browsing history's ranges, each asking where the menu was", async () => {
    commands.countHistory.mockResolvedValue({ status: "ok", data: 2 });
    await render();
    await click(button("Clear browsing history…"));
    await vi.waitFor(() => expect(openMenu).toHaveBeenCalled());
    const items = (openMenu.mock.calls[0]![0] as { items: MenuItem[] }).items;
    const hour = items.find((item) => item.id === "hour");
    expect(hour?.kind === "command" && hour.confirm?.action).toBe("Delete 2 visits");
  });

  it("gives a site row a menu, from a right-click or its actions button", async () => {
    commands.historySites.mockResolvedValue({
      status: "ok",
      data: [{ host: "a.com", visits: 2, lastVisitedAt: NOW }],
    });
    commands.countHistory.mockResolvedValue({ status: "ok", data: 2 });
    await render();
    await click(button("Sites"));
    const actions = container.querySelector("[aria-label='Actions for a.com']")!;
    await click(actions);
    await vi.waitFor(() => expect(openMenu).toHaveBeenCalled());
    const ids = (openMenu.mock.calls[0]![0] as { items: MenuItem[] }).items.map((item) => item.id);
    expect(ids).toEqual(["delete-site", "never"]);
    await act(async () => actions.closest("li")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true })));
    await vi.waitFor(() => expect(openMenu).toHaveBeenCalledTimes(2));
  });

  it("filters by address from one field, guessing the kind, and hides it when Hide is on", async () => {
    await render();
    await click(button("Filter by address"));
    const form = container.querySelector("form[aria-label='Filter by address']")!;
    await type(form.querySelector("input")!, "https://a.com/x");
    expect(form.querySelector<HTMLSelectElement>("select")?.value).toBe("page");
    const hide = button("Hide")!;
    await click(hide);
    expect(hide.getAttribute("aria-pressed")).toBe("true");
    await act(async () => form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
    await vi.waitFor(() => expect(lastFilter().exclude).toEqual([{ kind: "page", value: "https://a.com/x" }]));
    expect(lastFilter().include).toEqual([]);
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
