import { WindowControls } from "@features/window-controls/window-controls";
import { t } from "@shared/i18n";

/**
 * Shown in place of an internal page that failed to render, inside the
 * viewport, while the rest of the interface keeps working.
 */
export function PageCrash({ error, retry }: { error: unknown; retry: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <p className="font-medium">{t("error.title")}</p>
      <p className="text-text-secondary max-w-prose text-sm">{String(error)}</p>
      <button type="button" className="btn" onClick={retry}>
        {t("error.retry")}
      </button>
    </div>
  );
}

/**
 * Shown when the interface itself failed to render.
 *
 * It is only the top bar: below it, the content webview still owns the
 * rectangle last reported as the viewport, so anything drawn there would be
 * hidden under the page. The window controls stay, since the window has no
 * frame of its own to close it with.
 */
export function InterfaceCrash({ error }: { error: unknown }) {
  return (
    <header className="haku-bar flex h-8.5 shrink-0 items-center gap-2 pl-3" data-tauri-drag-region>
      <p className="min-w-0 flex-1 truncate text-sm" title={String(error)}>
        {t("error.interface")}
      </p>
      <button type="button" className="btn shrink-0" onClick={() => globalThis.location.reload()}>
        {t("error.reloadInterface")}
      </button>
      <WindowControls />
    </header>
  );
}
