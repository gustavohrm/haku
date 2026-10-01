import { InternalPage } from "@app/internal-page";
import { applyTheme, isThemePreference } from "@app/theme";
import { type Settings } from "@bindings";
import { commands } from "@ipc/commands";
import { useSettings } from "@ipc/hooks";
import { t } from "@shared/i18n";
import { useEffect, useState } from "react";

const MAX_CAPACITY = 12;

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

  /** @returns What Rust stored, which may differ from what was sent. */
  const update = async (patch: Partial<Settings>): Promise<Settings | null> => {
    const result = await commands.setSettings({ ...settings, ...patch });
    return result.status === "ok" ? result.data : null;
  };

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

      <section className="card stack">
        <h2 className="text-title-sm">{t("settings.performance")}</h2>
        <label className="field">
          <span className="label">{t("settings.capacity")}</span>
          <DraftInput
            type="number"
            min={1}
            max={MAX_CAPACITY}
            value={String(settings.webviewCapacity)}
            onCommit={async (next) => {
              const value = Number.parseInt(next, 10);
              if (!Number.isFinite(value)) {
                return null;
              }
              const stored = await update({ webviewCapacity: Math.min(Math.max(value, 1), MAX_CAPACITY) });
              return stored && String(stored.webviewCapacity);
            }}
          />
          <span className="hint">{t("settings.capacity.help")}</span>
        </label>
      </section>

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
    </InternalPage>
  );
}

interface DraftInputProps {
  type: "text" | "number";
  value: string;
  min?: number;
  max?: number;
  /** Saves the value, resolving to what was stored, or `null` if nothing was. */
  onCommit: (value: string) => Promise<string | null>;
}

/**
 * A field edited locally and saved when it is left or Enter is pressed.
 *
 * What was actually stored replaces the draft, so a value Rust corrected, such
 * as a blank home page restored to the default, shows as it really is. Escape
 * abandons the edit.
 */
function DraftInput({ type, value, min, max, onCommit }: DraftInputProps) {
  const [draft, setDraft] = useState(value);

  useEffect(() => setDraft(value), [value]);

  const commit = async () => {
    if (draft === value) {
      return;
    }
    setDraft((await onCommit(draft)) ?? value);
  };

  return (
    <input
      className="ipt"
      type={type}
      min={min}
      max={max}
      value={draft}
      spellCheck={false}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => void commit()}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          void commit();
        } else if (event.key === "Escape") {
          setDraft(value);
        }
      }}
    />
  );
}
