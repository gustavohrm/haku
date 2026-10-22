import { InternalPage } from "@app/internal-page";
import { applyTheme, isThemePreference } from "@app/theme";
import { type Settings } from "@bindings";
import { useSettings } from "@ipc/hooks";
import { updateSettings } from "@ipc/store";
import { t } from "@shared/i18n";

import { DraftInput } from "./draft-input";
import { ExtensionsSection } from "./extensions-section";
import { OptimizationSection } from "./optimization-section";

/**
 * Settings, rendered inside the chrome rather than in a webview.
 *
 * Changes are written through to Rust, which persists them and applies
 * anything with an immediate effect, such as destroying webviews when the pool
 * shrinks. Typed values are saved when the field is left or Enter is pressed,
 * not per keystroke: Rust sanitises what it receives, and a half-typed value
 * would be corrected out from under the person typing it.
 */
export function SettingsPage() {
  const settings = useSettings();
  if (!settings) {
    return null;
  }

  /**
   * @param change - A patch to merge into the settings, or a function computing the whole
   * new settings from the current ones. Prefer the function when the new value depends on
   * the old one, so that it reads the value as it is when the change is made.
   * @returns What Rust stored, which may differ from what was sent.
   */
  const update = (change: Partial<Settings> | ((current: Settings) => Settings)): Promise<Settings | null> =>
    updateSettings((current) => (typeof change === "function" ? change(current) : { ...current, ...change }));

  return (
    <InternalPage title={t("settings.title")}>
      <section className="card stack">
        <h2 className="text-title-sm">{t("settings.appearance")}</h2>
        <label className="field">
          <span className="label">{t("settings.theme")}</span>
          <select
            className="select"
            value={settings.theme}
            onChange={(event) => {
              const next = event.target.value;
              if (isThemePreference(next)) {
                applyTheme(next);
                void update({ theme: next });
              }
            }}
          >
            <option value="system">{t("settings.theme.system")}</option>
            <option value="light">{t("settings.theme.light")}</option>
            <option value="dark">{t("settings.theme.dark")}</option>
          </select>
        </label>
      </section>

      <OptimizationSection settings={settings} update={update} />

      <section className="card stack">
        <h2 className="text-title-sm">{t("settings.browsing")}</h2>
        <label className="field">
          <span className="label">{t("settings.search")}</span>
          <DraftInput
            type="text"
            value={settings.searchUrl}
            onCommit={async (next) => (await update({ searchUrl: next }))?.searchUrl ?? null}
          />
        </label>
        <label className="field">
          <span className="label">{t("settings.home")}</span>
          <DraftInput
            type="text"
            value={settings.homeUrl}
            onCommit={async (next) => (await update({ homeUrl: next }))?.homeUrl ?? null}
          />
        </label>
      </section>

      <ExtensionsSection settings={settings} update={update} />
    </InternalPage>
  );
}
