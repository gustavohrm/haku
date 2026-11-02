import { type HistoryRule, type Settings } from "@bindings";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { commands, feedback, tryUpdateSettings } = vi.hoisted(() => ({
  commands: {
    countHistory: vi.fn(),
    deleteHistory: vi.fn(async () => ({ status: "ok", data: 0 })),
    deleteHistoryVisits: vi.fn(async () => ({ status: "ok", data: 0 })),
    openTab: vi.fn(async () => ({ status: "ok", data: null })),
  },
  feedback: { info: vi.fn(), success: vi.fn(), dialog: { confirm: vi.fn() } },
  tryUpdateSettings: vi.fn(),
}));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/store", () => ({ tryUpdateSettings }));
vi.mock("@features/feedback/feedback", () => ({ feedback: () => feedback, reportUnexpected: vi.fn() }));

import { filterOf } from "./filter";
import { type VisitRow } from "./group";
import { clearMenu, confirmDelete, neverRecordSite, onHistoryChanged } from "./history-actions";
import { rowMenu } from "./row-menu";

const count = (data: number) => commands.countHistory.mockResolvedValue({ status: "ok", data });

beforeEach(() => {
  vi.clearAllMocks();
});

describe("confirmDelete", () => {
  const filter = filterOf({ text: "rust" });

  it("asks nothing and deletes nothing when the filter covers no visit", async () => {
    count(0);
    expect(await confirmDelete(filter, "Title")).toBe(false);
    expect(feedback.dialog.confirm).not.toHaveBeenCalled();
    expect(feedback.info).toHaveBeenCalled();
    expect(commands.deleteHistory).not.toHaveBeenCalled();
  });

  it("says how many visits go, and keeps them when declined", async () => {
    count(1234);
    feedback.dialog.confirm.mockResolvedValue(false);
    expect(await confirmDelete(filter, "Title")).toBe(false);
    expect(feedback.dialog.confirm.mock.calls[0]![0]).toContain((1234).toLocaleString());
    expect(commands.deleteHistory).not.toHaveBeenCalled();
  });

  it("deletes exactly the filter when confirmed, and tells lists to reload", async () => {
    count(2);
    feedback.dialog.confirm.mockResolvedValue(true);
    const changed = vi.fn();
    const stop = onHistoryChanged(changed);
    expect(await confirmDelete(filter, "Title")).toBe(true);
    stop();
    expect(commands.deleteHistory).toHaveBeenCalledWith(filter);
    expect(changed).toHaveBeenCalledOnce();
  });
});

describe("neverRecordSite", () => {
  const settings = (historyRules: HistoryRule[]) => ({ historyRules }) as Settings;
  const rule = (value: string, action: HistoryRule["action"]): HistoryRule => ({
    pattern: { kind: "site", value },
    action,
  });
  const site = filterOf({ include: [{ kind: "site", value: "a.com" }] });

  beforeEach(() => {
    tryUpdateSettings.mockResolvedValue({ status: "ok", data: {} });
  });

  it("puts the rule first, replacing one for the same site", async () => {
    count(0);
    await neverRecordSite("a.com");
    const change = tryUpdateSettings.mock.calls[0]![0] as (current: Settings) => Settings;
    expect(change(settings([rule("b.com", "always"), rule("a.com", "always")])).historyRules).toEqual([
      rule("a.com", "never"),
      rule("b.com", "always"),
    ]);
    expect(feedback.success).toHaveBeenCalled();
    expect(feedback.dialog.confirm).not.toHaveBeenCalled();
  });

  it("offers to delete what is recorded, and keeps it when dismissed", async () => {
    count(3);
    feedback.dialog.confirm.mockResolvedValue(false);
    await neverRecordSite("a.com");
    expect(commands.countHistory).toHaveBeenCalledWith(site);
    expect(commands.deleteHistory).not.toHaveBeenCalled();
  });

  it("deletes what is recorded when the offer is taken", async () => {
    count(3);
    feedback.dialog.confirm.mockResolvedValue(true);
    await neverRecordSite("a.com");
    expect(commands.deleteHistory).toHaveBeenCalledWith(site);
  });

  it("offers nothing when the rule could not be stored", async () => {
    tryUpdateSettings.mockResolvedValue({ status: "error", error: { kind: "InvalidPattern", message: "bad" } });
    await neverRecordSite("a.com");
    expect(commands.countHistory).not.toHaveBeenCalled();
  });
});

describe("clearMenu", () => {
  it("deletes from how far back the range reaches, or everything", async () => {
    count(1);
    feedback.dialog.confirm.mockResolvedValue(true);
    const items = clearMenu(() => 10_000_000);
    const choose = async (id: string) => {
      const item = items.find((each) => each.id === id);
      if (item?.kind !== "command") {
        throw new Error(id);
      }
      item.onSelect();
      await vi.waitFor(() => expect(commands.deleteHistory).toHaveBeenCalled());
      const [filter] = commands.deleteHistory.mock.calls.at(-1) as unknown as [{ from: number | null }];
      commands.deleteHistory.mockClear();
      return filter.from;
    };
    expect(await choose("hour")).toBe(10_000_000 - 60 * 60 * 1000);
    expect(await choose("all")).toBeNull();
  });
});

describe("rowMenu", () => {
  const row = (host: string): VisitRow => ({
    key: "k",
    url: host === "" ? "file:///C:/a.txt" : `https://${host}/a`,
    host,
    title: "A",
    visits: [{ id: 1, url: "", host, title: "A", visitedAt: 0, transition: "link" }],
  });
  const ids = (host: string, isPrivate: boolean) =>
    rowMenu(row(host), { isPrivate, showSite: vi.fn() }).map((item) => item.id);

  it("offers every action for a site in an ordinary window", () => {
    expect(ids("a.com", false)).toEqual([
      "open",
      "copy",
      "site",
      "delete-separator",
      "delete",
      "delete-page",
      "delete-site",
      "never",
    ]);
  });

  it("leaves out Never record in a private window", () => {
    expect(ids("a.com", true)).not.toContain("never");
  });

  it("leaves out the site actions for an address with no host", () => {
    expect(ids("", false)).toEqual(["open", "copy", "delete-separator", "delete", "delete-page"]);
  });

  it("deletes the row's visits, and every visit to its page", () => {
    const items = rowMenu(row("a.com"), { isPrivate: false, showSite: vi.fn() });
    const select = (id: string) => {
      const item = items.find((each) => each.id === id);
      if (item?.kind === "command") {
        item.onSelect();
      }
    };
    select("delete");
    expect(commands.deleteHistoryVisits).toHaveBeenCalledWith([1]);
    select("delete-page");
    expect(commands.deleteHistory).toHaveBeenCalledWith(
      filterOf({ include: [{ kind: "page", value: "https://a.com/a" }] }),
    );
  });
});
