import { type ListedSearchEngine, type Settings } from "@bindings";
import { commands } from "@ipc/commands";
import { useSettings } from "@ipc/hooks";
import { useEffect, useState } from "react";

/**
 * Every search engine, presets first, as Rust lists them, or null until
 * known. Asked again whenever settings change, since keywords, hidden
 * presets and custom engines are settings.
 * [Address bar § Search engines](../../../docs/specs/address-bar.md#search-engines).
 */
export function useSearchEngines(): ListedSearchEngine[] | null {
  const settings = useSettings();
  const [engines, setEngines] = useState<ListedSearchEngine[] | null>(null);
  useEffect(() => {
    if (settings === null) {
      return;
    }
    let isCurrent = true;
    const load = async () => {
      const result = await commands.searchEngines();
      if (isCurrent && result.status === "ok") {
        setEngines(result.data);
      }
    };
    void load();
    return () => {
      isCurrent = false;
    };
  }, [settings]);
  return engines;
}

/**
 * The name of the engine searches go to, or null until it is known.
 *
 * @param engines - What {@link useSearchEngines} gave.
 * @param settings - The settings naming the default engine.
 */
export function defaultEngineName(engines: ListedSearchEngine[] | null, settings: Settings | null): string | null {
  if (engines === null || settings === null) {
    return null;
  }
  return engines.find((listed) => listed.engine.id === settings.searchEngine)?.engine.name ?? null;
}
