import { events, type CloseConfirmationRequested } from "@bindings";
import { feedback } from "@features/feedback/feedback";
import { commands } from "@ipc/commands";
import { t, type TranslationKey } from "@shared/i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect } from "react";

type Wording = Readonly<Record<"title" | "one" | "many" | "close", TranslationKey>>;

const WORDING: Record<CloseConfirmationRequested["ending"], Wording> = {
  quit: {
    title: "downloads.confirm.quit",
    one: "downloads.confirm.quit.one",
    many: "downloads.confirm.quit.many",
    close: "downloads.confirm.quit.close",
  },
  privateSession: {
    title: "downloads.confirm.private",
    one: "downloads.confirm.one",
    many: "downloads.confirm.many",
    close: "downloads.confirm.close",
  },
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
      const wording = WORDING[payload.ending];
      const message =
        payload.downloads === 1 ? t(wording.one) : t(wording.many).replace("{count}", String(payload.downloads));
      const asked = feedback().dialog.confirm(message, {
        title: t(wording.title),
        type: "error",
        confirmLabel: t(wording.close),
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
