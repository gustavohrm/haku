import { events, type CloseConfirmationRequested } from "@bindings";
import { feedback } from "@features/feedback/feedback";
import { commands } from "@ipc/commands";
import { t, type TranslationKey } from "@shared/i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect } from "react";

const TITLES: Record<CloseConfirmationRequested["ending"], TranslationKey> = {
  quit: "downloads.confirm.quit",
  privateSession: "downloads.confirm.private",
};

/**
 * Asks before a close that would cancel downloads, which Rust held back
 * rather than made, and makes it once confirmed. Declining leaves everything
 * as it was.
 */
export function useCloseConfirmation(): void {
  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;
    let open: { dismiss(): void } | null = null;
    const ask = async (payload: CloseConfirmationRequested) => {
      // Asking twice at once, from the button and a shortcut, asks once.
      open?.dismiss();
      const message =
        payload.downloads === 1
          ? t("downloads.confirm.one")
          : t("downloads.confirm.many").replace("{count}", String(payload.downloads));
      const asked = feedback().dialog.confirm(message, {
        title: t(TITLES[payload.ending]),
        type: "error",
        confirmLabel: t("downloads.confirm.close"),
        cancelLabel: t("downloads.confirm.keep"),
      });
      open = asked;
      const confirmed: unknown = await asked;
      if (open === asked) {
        open = null;
      }
      if (confirmed === true) {
        await commands.confirmClose(payload.tab);
      }
    };
    const listen = async () => {
      // Rust sends it to this window only.
      const stop = await events.closeConfirmationRequested(getCurrentWebview()).listen(({ payload }) => {
        void ask(payload);
      });
      if (isMounted) {
        unlisten = stop;
      } else {
        stop();
      }
    };
    void listen();
    return () => {
      isMounted = false;
      unlisten?.();
      open?.dismiss();
    };
  }, []);
}
