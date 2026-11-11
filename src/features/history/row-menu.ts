import { type MenuItem } from "@features/overlays/menu-model";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";

import { filterOf } from "./filter";
import { type VisitRow } from "./group";
import {
  copyLink,
  deleteConfirmation,
  deleteNow,
  deleteVisits,
  HISTORY_RULES_URL,
  neverRecordOffer,
  neverRecordSite,
  recordSiteAgain,
  siteFilter,
  type SiteRule,
} from "./history-actions";

/** What a menu's site items need to know, read as the menu opens. */
export interface SiteMenuOptions {
  isPrivate: boolean;
  /**
   * The rule that decides the site's page, from `siteRuleFor`; `null` when it
   * could not be read, which offers Never record as if no rule did.
   */
  siteRule: SiteRule | null;
  /**
   * How many visits to the site are recorded, from `countVisits`; `null`
   * when that could not be read, which leaves out the items that say it.
   */
  siteVisits: number | null;
}

interface RowMenuOptions extends SiteMenuOptions {
  /** Narrows the page to the row's site. */
  showSite: (host: string) => void;
}

/**
 * A history row's menu. Deleting the row or every visit to its page goes at
 * once, since the item names exactly what goes; deleting a whole site asks
 * first, where the menu was, with its count. Site items are left out for an
 * address with no host.
 * [History § The history page](../../../docs/specs/history.md#the-history-page).
 */
export function rowMenu(row: VisitRow, { showSite, ...site }: RowMenuOptions): MenuItem[] {
  const items: MenuItem[] = [
    {
      kind: "command",
      id: "open",
      label: t("history.row.openInNewTab"),
      icon: "ic-plus",
      onSelect: () => void commands.openTab(row.url, true, "other"),
    },
    {
      kind: "command",
      id: "copy",
      label: t("history.row.copyLink"),
      icon: "ic-copy",
      onSelect: () => void copyLink(row.url),
    },
  ];
  if (row.host !== "") {
    items.push({
      kind: "command",
      id: "site",
      label: t("history.row.moreFromSite"),
      onSelect: () => showSite(row.host),
    });
  }
  items.push(
    { kind: "separator", id: "delete-separator" },
    {
      kind: "command",
      id: "delete",
      label: t("history.row.delete"),
      icon: "ic-trash-2",
      onSelect: () => void deleteVisits(row.visits.map((visit) => visit.id)),
    },
    {
      kind: "command",
      id: "delete-page",
      label: t("history.row.deletePage"),
      onSelect: () => void deleteNow(filterOf({ include: [{ kind: "page", value: row.url }] })),
    },
  );
  if (row.host !== "") {
    items.push(...siteItems(row.host, row.url, site));
  }
  return items;
}

/**
 * The items about a whole site, which a visit row's menu ends with and a row
 * of the Sites view offers alone: **Delete everything from this site…**,
 * which asks where the menu was with its count, then Never record or its
 * undo. Never record is left out of a private window, which records nothing
 * anyway.
 *
 * @param host - The site.
 * @param url - A page on it, for Record this site again to ask about.
 */
export function siteItems(host: string, url: string, { isPrivate, siteRule, siteVisits }: SiteMenuOptions): MenuItem[] {
  const items: MenuItem[] = [];
  if (siteVisits !== null) {
    items.push({
      kind: "command",
      id: "delete-site",
      label: t("history.row.deleteSite"),
      ...(siteVisits > 0 && { confirm: deleteConfirmation(siteVisits) }),
      onSelect: () => void deleteNow(siteFilter(host)),
    });
  }
  if (!isPrivate) {
    items.push(...neverItems(host, url, siteRule, siteVisits));
  }
  return items;
}

/**
 * Never record, or its undo when the site's own rule decides it. Where another
 * rule already stops the site, the way to the rules takes its place, since
 * adding a rule would change nothing and only settings can change that one.
 * Never record offers, as the settings form does, to delete what is already
 * recorded or keep it, as a submenu of the two; with nothing recorded, or no
 * count, it adds the rule at once.
 */
function neverItems(host: string, url: string, siteRule: SiteRule | null, siteVisits: number | null): MenuItem[] {
  if (siteRule?.kind === "own") {
    return [
      {
        kind: "command",
        id: "never",
        label: t("history.row.recordAgain"),
        icon: "ic-history",
        onSelect: () => void recordSiteAgain(host, url),
      },
    ];
  }
  if (siteRule?.kind === "other" && siteRule.rule.action === "never") {
    return [
      {
        kind: "command",
        id: "never",
        label: t("history.row.editRules"),
        icon: "ic-settings",
        onSelect: () => void commands.openTab(HISTORY_RULES_URL, true, "other"),
      },
    ];
  }
  if (siteVisits === null || siteVisits === 0) {
    return [
      {
        kind: "command",
        id: "never",
        label: t("history.row.neverRecord"),
        icon: "ic-circle-off",
        onSelect: () => void neverRecordSite(host, false),
      },
    ];
  }
  const offer = neverRecordOffer(siteVisits);
  return [
    {
      kind: "submenu",
      id: "never",
      label: t("history.row.neverRecord"),
      icon: "ic-circle-off",
      items: [
        { kind: "command", id: "never-keep", label: offer.keep, onSelect: () => void neverRecordSite(host, false) },
        { kind: "command", id: "never-delete", label: offer.delete, onSelect: () => void neverRecordSite(host, true) },
      ],
    },
  ];
}
