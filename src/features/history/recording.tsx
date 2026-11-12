import { commands } from "@ipc/commands";
import { useBrowserState, useSettings } from "@ipc/hooks";
import { updateSettings } from "@ipc/store";
import { t } from "@shared/i18n";
import { useId } from "react";

const setEnabled = (historyEnabled: boolean) => void updateSettings((current) => ({ ...current, historyEnabled }));
const setPaused = (paused: boolean) => void commands.setHistoryPaused(paused);

/**
 * The recording switch, stored across restarts, and Pause until restart,
 * which every window shows at once and nothing stores. Pause hides while
 * recording is paused: the banner says so, and resumes it.
 * [History § Recording on, off and paused](../../../docs/specs/history.md#recording-on-off-and-paused).
 */
export function RecordingControls() {
  const settings = useSettings();
  const { historyPaused } = useBrowserState();
  const switchId = useId();
  const enabled = settings?.historyEnabled ?? true;
  return (
    <div className="flex items-center gap-1">
      <label htmlFor={switchId} className="text-text-secondary me-1 text-sm">
        {t("history.recording")}
      </label>
      <input
        id={switchId}
        type="checkbox"
        className="switch"
        checked={enabled}
        disabled={settings === null}
        onChange={(event) => setEnabled(event.target.checked)}
      />
      {enabled && !historyPaused && (
        <button type="button" className="btn ghost text-text-secondary ms-1" onClick={() => setPaused(true)}>
          <i className="ic-pause ic-sm" aria-hidden="true" />
          {t("history.pause")}
        </button>
      )}
    </div>
  );
}

/** Says recording is off or paused, with the way to resume it. Nothing while it records. */
export function RecordingBanner() {
  const settings = useSettings();
  const { historyPaused } = useBrowserState();
  if (settings === null) {
    return null;
  }
  const off = !settings.historyEnabled;
  if (!off && !historyPaused) {
    return null;
  }
  return (
    <output className="card soft edgeless flex items-center justify-between gap-4 py-2 ps-4 pe-2 [--surface-padding:0]">
      <span className="flex items-center gap-3">
        <i className={off ? "ic-circle-off ic-md" : "ic-pause ic-md"} aria-hidden="true" />
        {off ? t("history.off") : t("history.paused")}
      </span>
      <button type="button" className="btn" onClick={() => (off ? setEnabled(true) : setPaused(false))}>
        {off ? t("history.turnOn") : t("history.resume")}
      </button>
    </output>
  );
}
