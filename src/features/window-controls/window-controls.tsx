import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Minimise, maximise and close.
 *
 * The window is undecorated so the interface can own its whole surface, which
 * means these have to be drawn and wired by hand. They keep the platform's
 * flat, full-height shape rather than the rounded controls elsewhere, because
 * that is where people expect to find them.
 */
export function WindowControls() {
  const appWindow = getCurrentWindow();

  return (
    <div className="flex shrink-0 items-stretch self-stretch">
      <WindowButton icon="ic-minus" label={t("window.minimize")} onClick={() => void appWindow.minimize()} />
      <WindowButton icon="ic-square" label={t("window.maximize")} onClick={() => void appWindow.toggleMaximize()} />
      <WindowButton icon="ic-x" label={t("window.close")} danger onClick={() => void appWindow.close()} />
    </div>
  );
}

interface WindowButtonProps {
  icon: string;
  label: string;
  danger?: boolean;
  onClick: () => void;
}

function WindowButton({ icon, label, danger, onClick }: WindowButtonProps) {
  return (
    <button
      type="button"
      className={cx(
        "grid w-11 place-items-center text-text-secondary transition-colors hover:text-text",
        danger ? "hover:bg-destructive hover:text-destructive-contrast" : "hover:bg-text/10",
      )}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      <i className={cx(icon, "ic-sm")} aria-hidden="true" />
    </button>
  );
}
