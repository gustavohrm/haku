import { type MenuItem } from "@features/overlays/menu-model";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";

import { filterOf } from "./filter";
import { type VisitRow } from "./group";
import { confirmDelete, copyLink, deleteNow, deleteVisits, neverRecordSite } from "./history-actions";

interface RowMenuOptions {
  isPrivate: boolean;
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
export function rowMenu(row: VisitRow, { isPrivate, showSite }: RowMenuOptions): MenuItem[] {
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
      items.push({
        kind: "command",
        id: "never",
        label: t("history.row.neverRecord"),
        icon: "ic-circle-off",
        onSelect: () => void neverRecordSite(row.host),
      });
    }
  }
  return items;
}
