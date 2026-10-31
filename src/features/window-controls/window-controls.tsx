import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { windowButtonClass } from "./window-button-class";

/**
 * Minimise, maximise and close.
 *
 * The window is undecorated so the interface can own its whole surface, which
 * means these have to be drawn and wired by hand. They keep the platform's
 * flat, full-height shape rather than the rounded controls elsewhere, because
 * that is where people expect to find them.
 */
export function WindowControls() {
  // Looked up on click, not while rendering: the interface's crash bar draws
  // these, and must not fail the same way when the window is out of reach.
  return (
    <div className="haku-window-controls flex shrink-0 items-stretch self-stretch">
      <WindowButton icon="ic-minus" label={t("window.minimize")} onClick={() => void getCurrentWindow().minimize()} />
      <WindowButton
        icon="ic-square"
        label={t("window.maximize")}
        onClick={() => void getCurrentWindow().toggleMaximize()}
      />
      <WindowButton icon="ic-x" label={t("window.close")} danger onClick={() => void getCurrentWindow().close()} />
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
    <button type="button" className={windowButtonClass(danger)} aria-label={label} title={label} onClick={onClick}>
      <i className={cx(icon, "ic-sm")} aria-hidden="true" />
    </button>
  );
}
