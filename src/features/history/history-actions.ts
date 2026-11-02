import { type HistoryFilter, type HistoryRule, type UrlPattern } from "@bindings";
import { feedback, reportUnexpected } from "@features/feedback/feedback";
import { type MenuItem } from "@features/overlays/menu-model";
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
 * Deletes what a filter covers, after a confirmation that says how many
 * visits go. Nothing is asked, or deleted, when the filter covers none.
 *
 * @param filter - What to delete.
 * @param title - The confirmation's title.
 * @returns Whether anything was deleted.
 */
export async function confirmDelete(filter: HistoryFilter, title: string): Promise<boolean> {
  try {
    const count = await unwrap(commands.countHistory(filter));
    if (count === 0) {
      feedback().info(t("history.delete.none"));
      return false;
    }
    const confirmed: unknown = await feedback().dialog.confirm(
      counted(count, "history.delete.confirm.one", "history.delete.confirm.many"),
      { title, type: "error", confirmLabel: t("history.delete"), cancelLabel: t("dialog.cancel") },
    );
    if (confirmed !== true) {
      return false;
    }
    await unwrap(commands.deleteHistory(filter));
    announce();
    return true;
  } catch (error) {
    reportUnexpected(error);
    return false;
  }
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
 * Deletes what a filter covers at once, without asking, for an action whose
 * own name says exactly what goes, such as deleting every visit to one page.
 */
export async function deleteNow(filter: HistoryFilter): Promise<void> {
  try {
    await unwrap(commands.deleteHistory(filter));
    announce();
  } catch (error) {
    reportUnexpected(error);
  }
}

/**
 * Adds a rule never to record `host`, at the top so it decides first, then,
 * when visits to it are already recorded, offers to delete them; dismissing
 * the offer keeps them. Choosing the menu item is the decision to stop
 * recording; only deleting asks. [History § Rules](../../../docs/specs/history.md#rules).
 *
 * @param host - The site never to record.
 */
export async function neverRecordSite(host: string): Promise<void> {
  const pattern: UrlPattern = { kind: "site", value: host };
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
  try {
    const filter = filterOf({ include: [pattern] });
    const count = await unwrap(commands.countHistory(filter));
    if (count === 0) {
      feedback().success(t("history.neverRecord.done").replace("{site}", () => host));
      return;
    }
    const confirmed: unknown = await feedback().dialog.confirm(
      counted(count, "history.neverRecord.offer.one", "history.neverRecord.offer.many"),
      {
        title: t("history.neverRecord.title").replace("{site}", () => host),
        type: "error",
        confirmLabel: counted(count, "history.neverRecord.delete.one", "history.neverRecord.delete.many"),
        cancelLabel: t("history.neverRecord.keep"),
      },
    );
    if (confirmed === true) {
      await unwrap(commands.deleteHistory(filter));
      announce();
    }
  } catch (error) {
    reportUnexpected(error);
  }
}

/** Where the history settings are: the Browsing section, at the History group's first row. */
export const HISTORY_SETTINGS_URL = "haku://settings/browsing#historyEnabled";

const siteRule = (host: string): UrlPattern => ({ kind: "site", value: host });
const isNeverRuleFor = (host: string) => (rule: HistoryRule) =>
  rule.action === "never" && samePattern(rule.pattern, siteRule(host));

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
 * Clear browsing history's menu: one item per range, each confirming how
 * many visits it deletes. Downloads stay.
 *
 * @param now - Read when a range is chosen, not when the menu opens.
 */
export function clearMenu(now: () => number = Date.now): MenuItem[] {
  return CLEAR_RANGES.map(({ id, label, reach }) => ({
    kind: "command",
    id,
    label: t(label),
    onSelect: () =>
      void confirmDelete(filterOf({ from: reach === null ? null : now() - reach }), t("history.clear.title")),
  }));
}
