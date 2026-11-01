import type { PermissionRequest, Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { t } from "@shared/i18n";
import { useEffect, useEffectEvent } from "react";

import { feedback } from "./feedback";
import { permissionTitle, toDecision } from "./permission-prompt";

/**
 * Shows the permission request the active tab is waiting on, and answers it.
 * [Permissions](../../../docs/specs/permissions.md).
 *
 * Only the active tab's request is shown, and only once its page dialog, if
 * any, is answered. If Rust withdraws the request first, because the page went
 * away, the one on screen is closed without sending an answer.
 *
 * @param tab - The active tab.
 */
export function usePermissionPrompt(tab: Tab | null): void {
  const pending = tab?.dialog ? null : (tab?.permission ?? null);
  // Read when the request changes, not a dependency: a new copy of the same
  // request must not show it again.
  const readPending = useEffectEvent(() => pending);

  const tabId = tab?.id;
  const requestId = pending?.id;

  useEffect(() => {
    const request = readPending();
    if (tabId === undefined || !request) {
      return;
    }

    let withdrawn = false;
    let escaped = false;
    // The toaster resolves Block and Escape alike. Only Escape raises the
    // dialog's `cancel` event, which does not bubble, so it is caught on the
    // way down; the marker class tells this request's dialog from another one
    // the toaster shows first.
    const marker = `haku-permission-${request.id}`;
    const onCancel = (event: Event) => {
      if (event.target instanceof Element && event.target.classList.contains(marker)) {
        escaped = true;
      }
    };
    document.addEventListener("cancel", onCancel, true);

    const handle = present(request, marker);
    const answer = async () => {
      const result = await handle;
      document.removeEventListener("cancel", onCancel, true);
      if (!withdrawn) {
        await commands.answerPermission(tabId, request.id, toDecision(result, escaped));
      }
    };
    void answer();

    return () => {
      withdrawn = true;
      document.removeEventListener("cancel", onCancel, true);
      handle.dismiss();
    };
  }, [tabId, requestId]);
}

function present(request: PermissionRequest, className: string) {
  return feedback().dialog.confirm(t("permission.message"), {
    title: permissionTitle(request),
    confirmLabel: t("permission.allow"),
    cancelLabel: t("permission.block"),
    className,
    // A stray click beside the dialog is neither an Allow nor a Block.
    backdropDismiss: false,
  });
}
