import { type HistoryRule, type Settings } from "@bindings";
import { type MenuItem } from "@features/overlays/menu-model";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { commands, feedback, tryUpdateSettings } = vi.hoisted(() => ({
  commands: {
    countHistory: vi.fn(),
    deleteHistory: vi.fn(async () => ({ status: "ok", data: 0 })),
    deleteHistoryVisits: vi.fn(async () => ({ status: "ok", data: 0 })),
    openTab: vi.fn(async () => ({ status: "ok", data: null })),
    historyRuleFor: vi.fn(async (): Promise<{ status: string; data?: HistoryRule | null }> => ({
      status: "ok",
      data: null,
    })),
  },
  feedback: { info: vi.fn(), success: vi.fn(), dialog: { confirm: vi.fn() } },
  tryUpdateSettings: vi.fn(),
}));
vi.mock("@ipc/commands", () => ({ commands }));
vi.mock("@ipc/store", () => ({ tryUpdateSettings }));
vi.mock("@features/feedback/feedback", () => ({ feedback: () => feedback, reportUnexpected: vi.fn() }));

import { filterOf } from "./filter";
import { type VisitRow } from "./group";
import {
  clearMenu,
  countVisits,
  deleteNow,
  neverRecordOffer,
  neverRecordSite,
  onHistoryChanged,
  recordSiteAgain,
  siteRuleFor,
  type SiteRule,
} from "./history-actions";
import { rowMenu, siteItems } from "./row-menu";

const count = (data: number) => commands.countHistory.mockResolvedValue({ status: "ok", data });

const command = (items: readonly MenuItem[], id: string) => {
  const item = items.find((each) => each.id === id);
  if (item?.kind !== "command") {
    throw new Error(id);
  }
  return item;
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("countVisits and deleteNow", () => {
  const filter = filterOf({ text: "rust" });

  it("counts what a filter covers, or nothing when the count failed", async () => {
    count(1234);
    expect(await countVisits(filter)).toBe(1234);
    commands.countHistory.mockResolvedValue({ status: "error" });
    expect(await countVisits(filter)).toBeNull();
  });

  it("deletes exactly the filter, and tells lists to reload", async () => {
    const changed = vi.fn();
    const stop = onHistoryChanged(changed);
    await deleteNow(filter);
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
    await neverRecordSite("a.com", false);
    const change = tryUpdateSettings.mock.calls[0]![0] as (current: Settings) => Settings;
    expect(change(settings([rule("b.com", "always"), rule("a.com", "always")])).historyRules).toEqual([
      rule("a.com", "never"),
      rule("b.com", "always"),
    ]);
    expect(feedback.success).toHaveBeenCalled();
    expect(feedback.dialog.confirm).not.toHaveBeenCalled();
  });

  it("keeps what is recorded when only from now on was chosen", async () => {
    await neverRecordSite("a.com", false);
    expect(commands.deleteHistory).not.toHaveBeenCalled();
  });

  it("deletes what is recorded when that was chosen", async () => {
    await neverRecordSite("a.com", true);
    expect(commands.deleteHistory).toHaveBeenCalledWith(site);
  });

  it("deletes nothing when the rule could not be stored", async () => {
    tryUpdateSettings.mockResolvedValue({ status: "error", error: { kind: "InvalidPattern", message: "bad" } });
    await neverRecordSite("a.com", true);
    expect(commands.deleteHistory).not.toHaveBeenCalled();
    expect(feedback.success).not.toHaveBeenCalled();
  });

  it("words the offer as the settings form does", () => {
    expect(neverRecordOffer(3)).toEqual({
      message: "3 recorded visits match this rule.",
      keep: "Only from now on",
      delete: "Never record and delete 3 visits",
    });
  });
});

describe("recordSiteAgain", () => {
  const rule = (value: string, action: HistoryRule["action"], kind: "site" | "wildcard" = "site"): HistoryRule => ({
    pattern: { kind, value },
    action,
  });

  const decides = (found: HistoryRule | null) =>
    commands.historyRuleFor.mockResolvedValue({ status: "ok", data: found });

  it("tells the site's own never rule from any other that decides it", async () => {
    decides(rule("a.com", "never"));
    expect(await siteRuleFor("https://a.com/x", "a.com")).toEqual({ kind: "own" });
    decides(rule("a.com", "addressOnly"));
    expect(await siteRuleFor("https://a.com/x", "a.com")).toEqual({
      kind: "other",
      rule: rule("a.com", "addressOnly"),
    });
    decides(rule("*a.com*", "never", "wildcard"));
    expect(await siteRuleFor("https://a.com/x", "a.com")).toEqual({
      kind: "other",
      rule: rule("*a.com*", "never", "wildcard"),
    });
    decides(null);
    expect(await siteRuleFor("https://a.com/x", "a.com")).toEqual({ kind: "none" });
    commands.historyRuleFor.mockResolvedValue({ status: "error" });
    expect(await siteRuleFor("https://a.com/x", "a.com")).toBeNull();
  });

  it("removes that rule and keeps every other", async () => {
    tryUpdateSettings.mockResolvedValue({ status: "ok", data: {} });
    decides(null);
    await recordSiteAgain("a.com", "https://a.com/x");
    const change = tryUpdateSettings.mock.calls[0]![0] as (current: Settings) => Settings;
    const others = [rule("a.com", "addressOnly"), rule("*a.com*", "never", "wildcard"), rule("b.com", "never")];
    expect(change({ historyRules: [rule("a.com", "never"), ...others] } as Settings).historyRules).toEqual(others);
    expect(feedback.success).toHaveBeenCalled();
  });

  it("names the rule that still stops the site", async () => {
    tryUpdateSettings.mockResolvedValue({ status: "ok", data: {} });
    decides(rule("*a.com*", "never", "wildcard"));
    await recordSiteAgain("a.com", "https://a.com/x");
    expect(feedback.success).not.toHaveBeenCalled();
    expect(feedback.info.mock.calls[0]![0]).toContain("*a.com*");
  });
});

describe("clearMenu", () => {
  const HOUR = 60 * 60 * 1000;

  it("counts each range from when the menu opens, and deletes from how far back it reaches", async () => {
    count(4);
    const items = await clearMenu(10_000_000);
    expect(commands.countHistory).toHaveBeenCalledWith(filterOf({ from: 10_000_000 - HOUR }));
    const hour = command(items, "hour");
    expect(hour.confirm).toEqual({
      message: "4 visits will be deleted. This can't be undone.",
      action: "Delete 4 visits",
      destructive: true,
    });
    hour.onSelect();
    await vi.waitFor(() => expect(commands.deleteHistory).toHaveBeenCalledWith(filterOf({ from: 10_000_000 - HOUR })));
    command(items, "all").onSelect();
    await vi.waitFor(() => expect(commands.deleteHistory).toHaveBeenLastCalledWith(filterOf({})));
  });

  it("asks nothing and deletes nothing for a range with no visit", async () => {
    count(0);
    const hour = command(await clearMenu(10_000_000), "hour");
    expect(hour.confirm).toBeUndefined();
    hour.onSelect();
    expect(feedback.info).toHaveBeenCalled();
    expect(commands.deleteHistory).not.toHaveBeenCalled();
  });

  it("disables a range whose count failed", async () => {
    commands.countHistory.mockResolvedValue({ status: "error" });
    expect(command(await clearMenu(10_000_000), "hour").disabled).toBe(true);
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
  const menu = (host: string, options: { isPrivate?: boolean; siteRule?: SiteRule | null; siteVisits?: number }) =>
    rowMenu(row(host), {
      isPrivate: options.isPrivate ?? false,
      siteRule: options.siteRule ?? null,
      siteVisits: options.siteVisits ?? 3,
      showSite: vi.fn(),
    });
  const ids = (host: string, isPrivate: boolean) => menu(host, { isPrivate }).map((item) => item.id);

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

  const neverItem = (siteRule: SiteRule) => menu("a.com", { siteRule }).find((each) => each.id === "never");

  it("offers Record this site again for a site with a never rule of its own", () => {
    const item = neverItem({ kind: "own" });
    expect(item?.kind === "command" && item.label).toBe("Record this site again");
  });

  it("leads to the history rules where another rule already stops the site", () => {
    const item = neverItem({
      kind: "other",
      rule: { pattern: { kind: "wildcard", value: "*a.com*" }, action: "never" },
    });
    expect(item?.kind === "command" && item.label).toBe("Edit history rules");
    expect(item?.kind === "command" && item.disabled).toBeFalsy();
    if (item?.kind === "command") {
      item.onSelect();
    }
    expect(commands.openTab).toHaveBeenCalledWith("haku://settings/browsing#historyRules", true, "other");
  });

  it("offers never recording from now on, or deleting what is recorded too, in the settings form's words", () => {
    const item = neverItem({ kind: "none" });
    expect(item?.kind).toBe("submenu");
    const choices = item?.kind === "submenu" ? item.items : [];
    expect(choices.map((each) => each.kind !== "separator" && each.label)).toEqual([
      "Only from now on",
      "Never record and delete 3 visits",
    ]);
  });

  it("adds the rule at once when nothing of the site is recorded", async () => {
    tryUpdateSettings.mockResolvedValue({ status: "ok", data: {} });
    const item = command(menu("a.com", { siteVisits: 0 }), "never");
    expect(item.label).toBe("Never record this site");
    item.onSelect();
    await vi.waitFor(() => expect(tryUpdateSettings).toHaveBeenCalled());
  });

  it("asks where the menu was before deleting everything from the site, saying how many visits go", () => {
    const item = command(menu("a.com", {}), "delete-site");
    expect(item.confirm).toEqual({
      message: "3 visits will be deleted. This can't be undone.",
      action: "Delete 3 visits",
      destructive: true,
    });
    item.onSelect();
    expect(commands.deleteHistory).toHaveBeenCalledWith(filterOf({ include: [{ kind: "site", value: "a.com" }] }));
  });

  it("leaves out Never record in a private window", () => {
    expect(ids("a.com", true)).not.toContain("never");
  });

  it("leaves out the site actions for an address with no host", () => {
    expect(ids("", false)).toEqual(["open", "copy", "delete-separator", "delete", "delete-page"]);
  });

  it("deletes the row's visits, and every visit to its page", () => {
    const items = menu("a.com", {});
    command(items, "delete").onSelect();
    expect(commands.deleteHistoryVisits).toHaveBeenCalledWith([1]);
    command(items, "delete-page").onSelect();
    expect(commands.deleteHistory).toHaveBeenCalledWith(
      filterOf({ include: [{ kind: "page", value: "https://a.com/a" }] }),
    );
  });
});

describe("siteItems", () => {
  const url = "https://a.com/";

  it("offers deleting everything from the site and never recording it", () => {
    const items = siteItems("a.com", url, { isPrivate: false, siteRule: { kind: "none" }, siteVisits: 2 });
    expect(items.map((item) => item.id)).toEqual(["delete-site", "never"]);
  });

  it("leaves out never recording in a private window, and deleting when the count failed", () => {
    expect(siteItems("a.com", url, { isPrivate: true, siteRule: null, siteVisits: 2 }).map((item) => item.id)).toEqual([
      "delete-site",
    ]);
    expect(siteItems("a.com", url, { isPrivate: true, siteRule: null, siteVisits: null })).toEqual([]);
  });
});
