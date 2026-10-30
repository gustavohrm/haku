import type { Download } from "@bindings";
import { locale, t, type TranslationKey } from "@shared/i18n";

const UNITS = ["byte", "kilobyte", "megabyte", "gigabyte"] as const;

/**
 * A file's size, in the largest unit it fills, as file managers show it.
 *
 * @param bytes - The size in bytes.
 */
export function formatSize(bytes: number): string {
  let unit = 0;
  let value = bytes;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return new Intl.NumberFormat(locale(), {
    style: "unit",
    unit: UNITS[unit],
    unitDisplay: "short",
    maximumFractionDigits: unit < 2 ? 0 : 1,
  }).format(value);
}

/** What can be done to a download as it is now, in the order it is offered. */
export type DownloadAction = "open" | "show" | "pause" | "resume" | "retry" | "cancel";

/**
 * The file's name, as the engine saved it.
 *
 * @param download - Any listed download.
 */
export function fileName(download: Download): string {
  return download.path.split(/[\\/]/).at(-1) || download.url;
}

/**
 * The actions a download offers. A failed one offers retry only when the
 * engine can carry on from where it stopped; Haku cannot start a download
 * again from its address.
 *
 * @param download - Any listed download.
 */
export function actionsFor(download: Download): DownloadAction[] {
  switch (download.state) {
    case "inProgress":
      return ["pause", "cancel"];
    case "paused":
      return download.canResume ? ["resume", "cancel"] : ["cancel"];
    case "completed":
      return ["open", "show"];
    case "failed":
      return download.canResume ? ["retry", "cancel"] : ["show"];
    case "cancelled":
      return [];
  }
}

const FAILURES: Record<NonNullable<Download["failure"]>, TranslationKey> = {
  network: "downloads.failed.network",
  server: "downloads.failed.server",
  file: "downloads.failed.file",
  lost: "downloads.failed.lost",
  other: "downloads.failed.other",
};

/**
 * One line on where a download stands: how much has arrived while it runs,
 * its size once done, and why when it failed.
 *
 * @param download - Any listed download.
 */
export function describeStatus(download: Download): string {
  const { received, total } = download;
  const progress =
    total === null
      ? formatSize(received)
      : t("downloads.progress")
          .replace("{received}", () => formatSize(received))
          .replace("{total}", () => formatSize(total));
  switch (download.state) {
    case "inProgress":
      return progress;
    case "paused":
      return `${t("downloads.paused")} · ${progress}`;
    case "completed":
      return formatSize(download.received);
    case "cancelled":
      return t("downloads.cancelled");
    case "failed":
      return t(FAILURES[download.failure ?? "other"]);
  }
}

/**
 * How far a download has come, from 0 to 1, or null when there is nothing to
 * show: its size is unknown, or it is no longer running.
 *
 * @param download - Any listed download.
 */
export function progressShare(download: Download): number | null {
  if ((download.state !== "inProgress" && download.state !== "paused") || !download.total) {
    return null;
  }
  return Math.min(1, download.received / download.total);
}
