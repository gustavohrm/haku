import type { PageDialog, Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";
import { useEffect, useRef } from "react";

import { feedback } from "./feedback";
import { dialogTitle, toAnswer } from "./page-dialog";

/**
 * Shows the dialog the active tab's page is paused on, and answers it.
 *
 * Only the active tab's dialog is shown; a background tab's waits until that
 * tab is selected. If Rust clears the dialog first, because the page went
 * away, the one on screen is closed without sending an answer: Rust has
 * already given one.
 *
 * @param tab - The active tab.
 */
export function usePageDialog(tab: Tab | null): void {
  const pending = tab?.dialog ?? null;
  const pendingRef = useRef<PageDialog | null>(pending);
  pendingRef.current = pending;

  const tabId = tab?.id;
  const dialogId = pending?.id;

  useEffect(() => {
    const dialog = pendingRef.current;
    if (tabId === undefined || !dialog) {
      return;
    }

    let withdrawn = false;
    const handle = present(dialog);
    const answer = async () => {
      const result: unknown = await handle;
      if (!withdrawn) {
        await commands.answerDialog(tabId, dialog.id, toAnswer(dialog.kind, result));
      }
    };
    void answer();

    return () => {
      withdrawn = true;
      handle.dismiss();
    };
  }, [tabId, dialogId]);
}

function present(dialog: PageDialog) {
  const { dialog: dialogs } = feedback();
  const title = dialogTitle(dialog.url);
  // A page cannot close its own dialog by a stray click beside it.
  const backdropDismiss = false;

  switch (dialog.kind) {
    case "alert":
      return dialogs.alert(dialog.message, { title, backdropDismiss });
    case "confirm":
      return dialogs.confirm(dialog.message, { title, backdropDismiss });
    case "prompt":
      return dialogs.prompt(dialog.message, { title, defaultValue: dialog.defaultText, backdropDismiss });
    case "beforeUnload":
      // Browsers no longer show a page's own text here, so neither does Haku.
      return dialogs.confirm(t("dialog.leave.message"), {
        title: t("dialog.leave.title"),
        confirmLabel: t("dialog.leave.confirm"),
        cancelLabel: t("dialog.leave.cancel"),
        backdropDismiss,
      });
  }
}
