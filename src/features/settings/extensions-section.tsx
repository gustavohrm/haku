import { type Settings } from "@bindings";
import { commands } from "@ipc/commands";
import { useExtensions } from "@ipc/hooks";
import { describeError } from "@ipc/result";
import { t } from "@shared/i18n";

interface ExtensionsSectionProps {
  settings: Settings;
  update: (change: (current: Settings) => Settings) => Promise<Settings | null>;
}

/**
 * Extensions installed from the extensions folder, each with a switch.
 *
 * Switching one off is stored as an exception in the settings, which Rust
 * applies to every page at once. Popups are opened from the toolbar.
 */
export function ExtensionsSection({ settings, update }: ExtensionsSectionProps) {
  const extensions = useExtensions();
  const disabled = settings.disabledExtensions;

  const setEnabled = (id: string, enabled: boolean) => {
    void update((current) => {
      const others = current.disabledExtensions.filter((other) => other !== id);
      return { ...current, disabledExtensions: enabled ? others : [...others, id] };
    });
  };

  return (
    <section className="card stack">
      <h2 className="text-title-sm">{t("settings.extensions")}</h2>

      {extensions !== null && extensions.error !== null && (
        <p className="text-text-secondary text-sm">
          {t("settings.extensions.error")} {describeError(extensions.error)}
        </p>
      )}

      {extensions?.installed.length === 0 && extensions.error === null && (
        <p className="text-text-secondary text-sm">{t("settings.extensions.empty")}</p>
      )}

      {extensions?.installed.map((extension) => (
        <div key={extension.id} className="flex items-center gap-1.5">
          <label htmlFor={`extension-${extension.id}`} className="min-w-0 flex-1 truncate">
            {extension.name}
          </label>
          <input
            id={`extension-${extension.id}`}
            type="checkbox"
            className="switch"
            checked={!disabled.includes(extension.id)}
            onChange={(event) => setEnabled(extension.id, event.target.checked)}
          />
        </div>
      ))}

      {extensions !== null && extensions.failed.length > 0 && (
        <p className="text-text-secondary text-sm">
          {t("settings.extensions.failed")} {extensions.failed.join(", ")}
        </p>
      )}

      <div className="flex flex-col gap-1">
        <button type="button" className="btn self-start" onClick={() => void commands.openExtensionsFolder()}>
          {t("settings.extensions.folder")}
        </button>
        <span className="hint">{t("settings.extensions.help")}</span>
      </div>
    </section>
  );
}
