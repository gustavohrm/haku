import { type HistoryFilter, type HistoryRule, type UrlPattern } from "@bindings";
import { feedback, reportUnexpected } from "@features/feedback/feedback";
import { type MenuConfirmation, type MenuItem } from "@features/overlays/menu-model";
import { commands } from "@ipc/commands";
import { unwrap } from "@ipc/result";
import { tryUpdateSettings } from "@ipc/store";
import { t, type TranslationKey } from "@shared/i18n";

import { filterOf } from "./filter";
import { describePattern, samePattern } from "./page-filter";

const listeners = new Set<() => void>();

/**
 * Listens for history this window deleted, so a list showing it reloads.
 *
 * @returns Stops listening.
 */
export function onHistoryChanged(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function announce() {
  for (const listener of listeners) {
    listener();
  }
}

/** `key.one` for one, otherwise `key.many` with `{count}` filled in. */
export function counted(count: number, one: TranslationKey, many: TranslationKey): string {
  return count === 1 ? t(one) : t(many).replace("{count}", count.toLocaleString());
}

/**
 * How many visits a filter covers, for a confirmation that says so.
 *
 * @returns The count, or `null` when it could not be read; the command
 * wrapper has already reported why.
 */
export async function countVisits(filter: HistoryFilter): Promise<number | null> {
  const result = await commands.countHistory(filter);
  return result.status === "ok" ? result.data : null;
}

/**
 * The question a step that deletes `count` visits asks where it started:
 * what goes, and **Delete N visits**. A menu takes it as an item's `confirm`.
 */
export function deleteConfirmation(count: number): Required<MenuConfirmation> {
  return {
    message: counted(count, "history.delete.confirm.one", "history.delete.confirm.many"),
    action: counted(count, "history.delete.count.one", "history.delete.count.many"),
    destructive: true,
  };
}

/**
 * Deletes visits at once, without asking: they were picked one by one.
 *
 * @param ids - The visits to delete.
 */
export async function deleteVisits(ids: number[]): Promise<void> {
  try {
    await unwrap(commands.deleteHistoryVisits(ids));
    announce();
  } catch (error) {
    reportUnexpected(error);
  }
}

/**
 * Deletes what a filter covers at once, without asking here: for an action
 * whose own name says exactly what goes, such as deleting every visit to one
 * page, or one that has already asked, with {@link deleteConfirmation}.
 */
export async function deleteNow(filter: HistoryFilter): Promise<void> {
  try {
    await unwrap(commands.deleteHistory(filter));
    announce();
  } catch (error) {
    reportUnexpected(error);
  }
}

const siteRule = (host: string): UrlPattern => ({ kind: "site", value: host });
const isNeverRuleFor = (host: string) => (rule: HistoryRule) =>
  rule.action === "never" && samePattern(rule.pattern, siteRule(host));

/** The filter covering every visit to `host` and its subdomains. */
export function siteFilter(host: string): HistoryFilter {
  return filterOf({ include: [siteRule(host)] });
}

/**
 * Adds a rule never to record `host`, at the top so it decides first, then
 * deletes the visits to it already recorded when that was chosen. The choice
 * is offered before, where the item was, with {@link neverRecordOffer}, as
 * the settings form offers it; nothing is deleted without it.
 * [History § Rules](../../../docs/specs/history.md#rules).
 *
 * @param host - The site never to record.
 * @param deleteRecorded - Whether to delete the visits already recorded.
 */
export async function neverRecordSite(host: string, deleteRecorded: boolean): Promise<void> {
  const pattern = siteRule(host);
  const stored = await tryUpdateSettings((current) => ({
    ...current,
    historyRules: [
      { pattern, action: "never" },
      ...current.historyRules.filter((rule) => !samePattern(rule.pattern, pattern)),
    ],
  }));
  if (stored?.status !== "ok") {
    return;
  }
  feedback().success(t("history.neverRecord.done").replace("{site}", () => host));
  if (deleteRecorded) {
    await deleteNow(siteFilter(host));
  }
}

/**
 * The offer to delete the `count` visits already recorded, worded as the
 * settings form words it for a new `never` rule: what is recorded, then
 * **Only from now on** or **Never record and delete N visits**.
 */
export function neverRecordOffer(count: number): { message: string; keep: string; delete: string } {
  return {
    message: counted(count, "settings.historyRules.offer.one", "settings.historyRules.offer.many"),
    keep: t("settings.historyRules.offer.keep"),
    delete: counted(count, "settings.historyRules.offer.delete.one", "settings.historyRules.offer.delete.many"),
  };
}

/** Where the history settings are: the Browsing section, at the History group's first row. */
export const HISTORY_SETTINGS_URL = "haku://settings/browsing#historyEnabled";

/** The history rules in settings, where a rule that stops a site is changed. */
export const HISTORY_RULES_URL = "haku://settings/browsing#historyRules";

/**
 * What the rules say about a site, from the rule that decides its page:
 * nothing, so it is recorded; its own never rule, as "Never record this
 * site…" adds, which can be undone; or another rule, named so the menu can
 * say why. [History § Elsewhere](../../../docs/specs/history.md#elsewhere).
 */
export type SiteRule = { kind: "none" } | { kind: "own" } | { kind: "other"; rule: HistoryRule };

/**
 * Asks Rust which rule decides `url`, whose site is `host`, with the matcher
 * recording uses, so the menu and recording cannot disagree.
 *
 * @returns What decides it, or `null` when that could not be read; the
 * command wrapper has already reported why.
 */
export async function siteRuleFor(url: string, host: string): Promise<SiteRule | null> {
  const result = await commands.historyRuleFor(url);
  if (result.status === "error") {
    return null;
  }
  const rule = result.data;
  if (rule === null) {
    return { kind: "none" };
  }
  return isNeverRuleFor(host)(rule) ? { kind: "own" } : { kind: "other", rule };
}

const RULE_NOTES: Record<HistoryRule["action"], TranslationKey> = {
  never: "history.rule.never",
  addressOnly: "history.rule.addressOnly",
  always: "history.rule.always",
};

/** Says what another rule does to the site, naming its pattern. */
export function ruleNote(rule: HistoryRule): string {
  return t(RULE_NOTES[rule.action]).replace("{rule}", () => describePattern(rule.pattern));
}

/**
 * Removes the rule never to record `host`, undoing "Never record this site…",
 * then says whether the site is recorded again or another rule still stops it.
 *
 * @param host - The site to record again.
 * @param url - A page on it, to ask which rule decides it now.
 */
export async function recordSiteAgain(host: string, url: string): Promise<void> {
  const stored = await tryUpdateSettings((current) => ({
    ...current,
    historyRules: current.historyRules.filter((rule) => !isNeverRuleFor(host)(rule)),
  }));
  if (stored?.status !== "ok") {
    return;
  }
  const now = await siteRuleFor(url, host);
  if (now?.kind === "other" && now.rule.action === "never") {
    feedback().info(
      t("history.recordAgain.stillStopped")
        .replace("{site}", () => host)
        .replace("{rule}", () => describePattern(now.rule.pattern)),
    );
  } else {
    feedback().success(t("history.recordAgain.done").replace("{site}", () => host));
  }
}

/** Copies a link to the clipboard. */
export async function copyLink(url: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(url);
  } catch (error) {
    reportUnexpected(error);
  }
}

const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;

/** The ranges Clear browsing history offers, as how far back each reaches. */
export const CLEAR_RANGES: { id: string; label: TranslationKey; reach: number | null }[] = [
  { id: "hour", label: "history.clear.hour", reach: HOUR_MS },
  { id: "day", label: "history.clear.day", reach: DAY_MS },
  { id: "week", label: "history.clear.week", reach: 7 * DAY_MS },
  { id: "weeks4", label: "history.clear.weeks4", reach: 28 * DAY_MS },
  { id: "all", label: "history.clear.all", reach: null },
];

/**
 * Clear browsing history's menu: one item per range, each asking where the
 * menu was, with how many visits it deletes, counted as the menu opens. A
 * range with nothing in it says so instead, and one whose count failed, which
 * the command wrapper reported, is disabled. Downloads stay.
 *
 * @param now - When the menu opens, which every range reaches back from.
 */
export async function clearMenu(now: number = Date.now()): Promise<MenuItem[]> {
  return Promise.all(
    CLEAR_RANGES.map(async ({ id, label, reach }): Promise<MenuItem> => {
      const filter = filterOf({ from: reach === null ? null : now - reach });
      const count = await countVisits(filter);
      return {
        kind: "command",
        id,
        label: t(label),
        disabled: count === null,
        ...(count !== null && count > 0 && { confirm: deleteConfirmation(count) }),
        onSelect: () => (count === 0 ? feedback().info(t("history.delete.none")) : void deleteNow(filter)),
      };
    }),
  );
}
