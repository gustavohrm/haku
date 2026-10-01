/**
 * Translation lookup.
 *
 * Deliberately minimal: Haku ships English only for now, but every string the
 * interface shows goes through `t` from the first commit. Adding Portuguese and
 * Spanish then means adding dictionaries and a locale setting, not finding and
 * replacing every literal in the codebase.
 */

import { en, type Dictionary, type TranslationKey } from "./en";

export type { Dictionary, TranslationKey };

export const DEFAULT_LOCALE = "en";

const dictionaries: Record<string, Dictionary> = { en };

let active: Dictionary = en;
let activeLocale = DEFAULT_LOCALE;

/** The locale currently in use, which may not be the one that was requested. */
export function locale(): string {
  return activeLocale;
}

/**
 * Switches locale, falling back to English when the locale is not available.
 *
 * @param next - BCP-47 language tag to switch to.
 * @returns The locale actually in use afterwards.
 */
export function setLocale(next: string): string {
  const dictionary = dictionaries[next];
  active = dictionary ?? en;
  activeLocale = dictionary ? next : DEFAULT_LOCALE;
  return activeLocale;
}

/**
 * Translates a key.
 *
 * @param key - Key to look up.
 * @returns The translated string, or the key itself if a dictionary is missing it.
 */
export function t(key: TranslationKey): string {
  return active[key] ?? en[key] ?? key;
}
