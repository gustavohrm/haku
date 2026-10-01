import { createTheme } from "@codenhub/theme";

/**
 * Theme handling.
 *
 * `@codenhub/theme` owns preference storage, the system-preference listener and
 * the pre-paint script that avoids a flash of the wrong theme. Haku only decides
 * when to follow the system and when to override it.
 */
const theme = createTheme({
  storageKey: "haku:theme",
  attribute: "data-theme",
  isTailwindCss: true,
});

export type ThemePreference = "system" | "light" | "dark";

export function initTheme(): void {
  theme.init();
}

/**
 * Applies a theme preference.
 *
 * @param preference - `system` follows the operating system; the others pin it.
 */
export function applyTheme(preference: ThemePreference): void {
  if (preference === "system") {
    theme.clearPreference();
    return;
  }
  theme.set(preference);
}

export function isThemePreference(value: string): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}
