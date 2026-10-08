import { events, type Extensions, type Settings } from "@bindings";
import { commands } from "@ipc/commands";
import { unwrap } from "@ipc/result";
import { t } from "@shared/i18n";
import { useEffect, useState } from "react";

interface ExtensionsSectionProps {
  settings: Settings;
  update: (patch: Partial<Settings>) => Promise<Settings | null>;
}

/**
 * Extensions installed from the extensions folder, each with a switch.
 *
 * Switching one off is stored as an exception in the settings, which Rust
 * applies to every page at once. Each extension's popup opens as a tab,
 * because the engine draws no toolbar to open it from.
 */
export function ExtensionsSection({ settings, update }: ExtensionsSectionProps) {
  const extensions = useExtensions();
  const disabled = settings.disabledExtensions;

  const setEnabled = (id: string, enabled: boolean) => {
    const others = disabled.filter((other) => other !== id);
    void update({ disabledExtensions: enabled ? others : [...others, id] });
  };

  return (
    <section className="card stack">
      <h2 className="text-title-sm">{t("settings.extensions")}</h2>

      {extensions?.installed.length === 0 && (
        <p className="text-text-secondary text-sm">{t("settings.extensions.empty")}</p>
      )}

      {extensions?.installed.map((extension) => (
        <div key={extension.id} className="flex items-center gap-1.5">
          <label htmlFor={`extension-${extension.id}`} className="min-w-0 flex-1 truncate">
            {extension.name}
          </label>
          {extension.popup !== null && (
            <button
              type="button"
              className="btn ghost"
              onClick={() => extension.popup !== null && void commands.openTab(extension.popup, true)}
            >
              {t("settings.extensions.open")}
            </button>
          )}
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

function useExtensions(): Extensions | null {
  const [extensions, setExtensions] = useState<Extensions | null>(null);

  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;

    const connect = async () => {
      // Listening first: the folder is installed at startup, possibly while
      // this page is already open.
      const stop = await events.extensionsChanged.listen((event) => {
        if (isMounted) {
          setExtensions(event.payload);
        }
      });
      if (!isMounted) {
        stop();
        return;
      }
      unlisten = stop;
      const first = await unwrap(commands.extensions());
      if (isMounted) {
        setExtensions((current) => current ?? first);
      }
    };
    void connect();

    return () => {
      isMounted = false;
      unlisten?.();
    };
  }, []);

  return extensions;
}
