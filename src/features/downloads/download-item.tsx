import type { Download } from "@bindings";
import { commands } from "@ipc/commands";
import { cx } from "@shared/class-names";
import { t, type TranslationKey } from "@shared/i18n";
import { memo, type CSSProperties } from "react";

import { actionsFor, describeStatus, fileName, progressShare, type DownloadAction } from "./describe";

const ACTIONS: Record<DownloadAction, { icon: string; label: TranslationKey; run: (download: Download) => void }> = {
  open: {
    icon: "ic-external-link",
    label: "downloads.open",
    run: (download) => void commands.openDownload(download.id),
  },
  show: { icon: "ic-folder-open", label: "downloads.show", run: (download) => void commands.showDownload(download.id) },
  pause: { icon: "ic-pause", label: "downloads.pause", run: (download) => void commands.pauseDownload(download.id) },
  resume: { icon: "ic-play", label: "downloads.resume", run: (download) => void commands.resumeDownload(download.id) },
  retry: {
    icon: "ic-rotate-ccw",
    label: "downloads.retry",
    run: (download) => void commands.resumeDownload(download.id),
  },
  cancel: { icon: "ic-x", label: "downloads.cancel", run: (download) => void commands.cancelDownload(download.id) },
};

/**
 * One download: its name, where it stands, and what can be done to it.
 *
 * A finished file opens from its name, the largest target in the row, and
 * its **Show in folder** shows on the row's hover or focus. What a running,
 * paused or failed download offers stays in view, a failed one's **Show in
 * folder** included: it is why the row is being looked at ([Interface guidelines § Lists](../../../docs/guidelines/interface.md#lists)).
 *
 * Opening the file, or its folder, is asked of Rust, which only opens a path
 * it recorded for this download. Re-rendered only when its own download
 * changed, so a running download redraws one entry of a long list.
 */
export const DownloadItem = memo(function DownloadItem({ download }: { download: Download }) {
  const share = progressShare(download);
  const name = fileName(download);
  const failed = download.state === "failed";
  const actions = actionsFor(download);
  const opens = actions.includes("open");

  return (
    <li className="haku-row group min-h-8 py-1.5">
      <i className="ic-file ic-sm text-text-secondary shrink-0" aria-hidden="true" />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        {opens ? (
          <button
            type="button"
            className="truncate text-left font-medium hover:underline focus-visible:underline"
            aria-label={`${t("downloads.open")}: ${name}`}
            title={download.path}
            onClick={() => ACTIONS.open.run(download)}
          >
            {name}
          </button>
        ) : (
          <span
            className={cx("truncate font-medium", download.state === "cancelled" && "text-text-secondary line-through")}
            title={download.path}
          >
            {name}
          </span>
        )}
        {(share !== null || download.state === "inProgress") && (
          <div
            // The design system draws `.progress` on a box with its fill as a
            // pseudo-element, which a native <progress> does not take.
            // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role
            role="progressbar"
            className={cx("progress h-1", share === null && "indeterminate")}
            aria-label={`${t("downloads.progressLabel")}: ${name}`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={share === null ? undefined : Math.round(share * 100)}
            style={share === null ? undefined : ({ "--progress-value": `${share * 100}%` } as CSSProperties)}
          />
        )}
        <span className={cx("truncate text-sm", failed ? "text-destructive" : "text-text-secondary")}>
          {describeStatus(download)}
        </span>
      </div>
      <div className="flex shrink-0 gap-0.5">
        {actions.map((action) => {
          if (action === "open") {
            return null;
          }
          const { icon, label, run } = ACTIONS[action];
          return (
            <button
              key={action}
              type="button"
              // Beside a name that opens the file, Show in folder is not what the row is about, so it waits for the row's hover or focus.
              className={cx(
                "btn icon ghost",
                action === "show" && opens && "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100",
              )}
              aria-label={`${t(label)}: ${name}`}
              title={t(label)}
              onClick={() => run(download)}
            >
              <i className={cx(icon, "ic-sm")} aria-hidden="true" />
            </button>
          );
        })}
      </div>
    </li>
  );
});
