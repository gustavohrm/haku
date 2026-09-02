import { commands, type HistoryEntry } from "@bindings";
import { unwrap } from "@ipc/result";
import { t } from "@shared/i18n";
import { useCallback, useEffect, useState } from "react";

const PAGE_SIZE = 200;

/**
 * Recent history.
 *
 * A deliberately plain list for now. Rust has recorded visits since the first
 * release, so the filtering and grouping the full history feature calls for can
 * be built on top of real data rather than starting from an empty table.
 */
export function HistoryPage() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);

  const load = useCallback(async () => {
    setEntries(await unwrap(commands.recentHistory(PAGE_SIZE)));
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="haku-page">
      <div className="flex items-center justify-between gap-4">
        <h1 className="haku-page-title">{t("history.title")}</h1>
        <button
          type="button"
          className="haku-button"
          onClick={async () => {
            await unwrap(commands.clearHistory());
            await load();
          }}
        >
          {t("history.clear")}
        </button>
      </div>

      {entries.length === 0 ? (
        <p className="haku-help">{t("history.empty")}</p>
      ) : (
        <ul className="haku-history">
          {entries.map((entry) => (
            <li key={entry.id}>
              <button type="button" className="haku-history-row" onClick={() => void commands.openTab(entry.url, true)}>
                <span className="truncate font-medium">{entry.title || entry.url}</span>
                <span className="haku-history-url truncate">{entry.url}</span>
                <time dateTime={new Date(entry.visitedAt).toISOString()}>
                  {new Date(entry.visitedAt).toLocaleString()}
                </time>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
