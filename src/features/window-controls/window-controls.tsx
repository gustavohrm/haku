import { t } from "@shared/i18n";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Minimise, maximise and close.
 *
 * The window is undecorated so the interface can own its whole surface, which
 * means these have to be drawn and wired by hand.
 */
export function WindowControls() {
  const appWindow = getCurrentWindow();

  return (
    <div className="flex shrink-0 items-stretch self-stretch">
      <button
        type="button"
        className="haku-window-button"
        aria-label={t("window.minimize")}
        title={t("window.minimize")}
        onClick={() => void appWindow.minimize()}
      >
        <i className="ic-minus" aria-hidden="true" />
      </button>
      <button
        type="button"
        className="haku-window-button"
        aria-label={t("window.maximize")}
        title={t("window.maximize")}
        onClick={() => void appWindow.toggleMaximize()}
      >
        <i className="ic-square" aria-hidden="true" />
      </button>
      <button
        type="button"
        className="haku-window-button haku-window-close"
        aria-label={t("window.close")}
        title={t("window.close")}
        onClick={() => void appWindow.close()}
      >
        <i className="ic-x" aria-hidden="true" />
      </button>
    </div>
  );
}
