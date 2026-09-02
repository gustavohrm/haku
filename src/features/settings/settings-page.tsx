import { applyTheme, isThemePreference } from "@app/theme";
import { commands } from "@bindings";
import { useSettings } from "@ipc/hooks";
import { t } from "@shared/i18n";

const MAX_CAPACITY = 12;

/**
 * Settings, rendered inside the chrome rather than in a webview.
 *
 * Every change is written straight through to Rust, which persists it and
 * applies anything with an immediate effect, such as destroying webviews when
 * the pool shrinks.
 */
export function SettingsPage() {
  const settings = useSettings();
  if (!settings) {
    return null;
  }

  const update = (patch: Partial<typeof settings>) => {
    void commands.setSettings({ ...settings, ...patch });
  };

  return (
    <div className="haku-page">
      <h1 className="haku-page-title">{t("settings.title")}</h1>

      <section className="haku-section">
        <h2 className="haku-section-title">{t("settings.appearance")}</h2>
        <label className="haku-field">
          <span>{t("settings.theme")}</span>
          <select
            value={settings.theme}
            onChange={(event) => {
              const next = event.target.value;
              if (isThemePreference(next)) {
                applyTheme(next);
                update({ theme: next });
              }
            }}
          >
            <option value="system">{t("settings.theme.system")}</option>
            <option value="light">{t("settings.theme.light")}</option>
            <option value="dark">{t("settings.theme.dark")}</option>
          </select>
        </label>
      </section>

      <section className="haku-section">
        <h2 className="haku-section-title">{t("settings.performance")}</h2>
        <label className="haku-field">
          <span>{t("settings.capacity")}</span>
          <input
            type="number"
            min={1}
            max={MAX_CAPACITY}
            value={settings.webviewCapacity}
            onChange={(event) => {
              const value = Number.parseInt(event.target.value, 10);
              if (Number.isFinite(value)) {
                update({ webviewCapacity: Math.min(Math.max(value, 1), MAX_CAPACITY) });
              }
            }}
          />
        </label>
        <p className="haku-help">{t("settings.capacity.help")}</p>
      </section>

      <section className="haku-section">
        <label className="haku-field">
          <span>{t("settings.search")}</span>
          <input
            type="text"
            value={settings.searchUrl}
            spellCheck={false}
            onChange={(event) => update({ searchUrl: event.target.value })}
          />
        </label>
        <label className="haku-field">
          <span>{t("settings.home")}</span>
          <input
            type="text"
            value={settings.homeUrl}
            spellCheck={false}
            onChange={(event) => update({ homeUrl: event.target.value })}
          />
        </label>
      </section>
    </div>
  );
}
