import { type MenuItem } from "@features/overlays/menu-model";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";

import { filterOf } from "./filter";
import { type VisitRow } from "./group";
import {
  confirmDelete,
  copyLink,
  deleteNow,
  deleteVisits,
  neverRecordSite,
  recordSiteAgain,
  ruleNote,
  type SiteRule,
} from "./history-actions";

interface RowMenuOptions {
  isPrivate: boolean;
  /**
   * The rule that decides the row's page, from `siteRuleFor`; `null` when it
   * could not be read, which offers Never record as if no rule did.
   */
  siteRule: SiteRule | null;
  /** Narrows the page to the row's site. */
  showSite: (host: string) => void;
}

/**
 * A history row's menu. Deleting the row or every visit to its page goes at
 * once, since the item names exactly what goes; deleting a whole site asks
 * first with its count. Site items are left out for an address with no host,
 * and Never record for a private window, which records nothing anyway.
 * [History § The history page](../../../docs/specs/history.md#the-history-page).
 */
export function rowMenu(row: VisitRow, { isPrivate, siteRule, showSite }: RowMenuOptions): MenuItem[] {
  const site = { kind: "site", value: row.host } as const;
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
    items.push({
      kind: "command",
      id: "delete-site",
      label: t("history.row.deleteSite"),
      onSelect: () =>
        void confirmDelete(
          filterOf({ include: [site] }),
          t("history.row.deleteSite.title").replace("{site}", () => row.host),
        ),
    });
    if (!isPrivate) {
      items.push(neverItem(row, siteRule));
    }
  }
  return items;
}

/**
 * Never record, or its undo when the site's own rule decides it. Another rule
 * that already stops the site is named instead, disabled, since adding a rule
 * would change nothing and only settings can change that one.
 */
function neverItem(row: VisitRow, siteRule: SiteRule | null): MenuItem {
  if (siteRule?.kind === "own") {
    return {
      kind: "command",
      id: "never",
      label: t("history.row.recordAgain"),
      icon: "ic-history",
      onSelect: () => void recordSiteAgain(row.host, row.url),
    };
  }
  if (siteRule?.kind === "other" && siteRule.rule.action === "never") {
    return { kind: "command", id: "never", label: ruleNote(siteRule.rule), disabled: true, onSelect: () => {} };
  }
  return {
    kind: "command",
    id: "never",
    label: t("history.row.neverRecord"),
    icon: "ic-circle-off",
    onSelect: () => void neverRecordSite(row.host),
  };
}
