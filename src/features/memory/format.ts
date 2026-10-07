import { locale } from "@shared/i18n";

const MB = 1024 * 1024;
const GB = 1024 * MB;

/**
 * A byte count in the unit people read memory in.
 *
 * @param bytes - Bytes, or `null` when the figure is unknown.
 * @returns Megabytes below a gigabyte, gigabytes to one decimal above, or a dash.
 */
export function formatBytes(bytes: number | null): string {
  if (bytes === null) {
    return "—";
  }
  if (bytes >= GB) {
    return `${new Intl.NumberFormat(locale(), { maximumFractionDigits: 1 }).format(bytes / GB)} GB`;
  }
  return `${new Intl.NumberFormat(locale(), { maximumFractionDigits: 0 }).format(bytes / MB)} MB`;
}

/**
 * A share from 0 to 1 as a percentage.
 *
 * @param share - The share, or `null` when it is unknown.
 */
export function formatShare(share: number | null): string {
  if (share === null) {
    return "—";
  }
  return new Intl.NumberFormat(locale(), { style: "percent", maximumFractionDigits: 0 }).format(share);
}

/**
 * Adds up figures that may be unknown.
 *
 * @returns The sum of the known figures, or `null` when none is known.
 */
export function sumKnown(values: (number | null)[]): number | null {
  const known = values.filter((value): value is number => value !== null);
  return known.length === 0 ? null : known.reduce((total, value) => total + value, 0);
}
