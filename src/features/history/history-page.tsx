import { InternalPage } from "@app/internal-page";
import { type HistoryEntry } from "@bindings";
import { reportUnexpected } from "@features/feedback/feedback";
import { commands } from "@ipc/commands";
import { unwrap } from "@ipc/result";
import { t } from "@shared/i18n";
import { useEffect, useState } from "react";

const PAGE_SIZE = 200;

const recentHistory = () => unwrap(commands.recentHistory(PAGE_SIZE));

/**
 * Recent history.
 *
 * A deliberately plain list for now. Rust has recorded visits since the first
 * release, so the filtering and grouping the full history feature calls for can
 * be built on top of real data rather than starting from an empty table.
 */
export function HistoryPage() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);

  const load = async () => {
    try {
      setEntries(await recentHistory());
    } catch (error) {
      reportUnexpected(error);
    }
  };

  useEffect(() => {
    let current = true;
    const loadFirst = async () => {
      try {
        const entries = await recentHistory();
        if (current) {
          setEntries(entries);
        }
      } catch (error) {
        reportUnexpected(error);
      }
    };
    void loadFirst();
    return () => {
      current = false;
    };
  }, []);

  return (
    <InternalPage
      title={t("history.title")}
      actions={
        <button
          type="button"
          className="btn destructive ghost edged"
          disabled={entries.length === 0}
          onClick={async () => {
            try {
              await unwrap(commands.clearHistory());
            } catch (error) {
              reportUnexpected(error);
              return;
            }
            await load();
          }}
        >
          {t("history.clear")}
        </button>
      }
    >
      {entries.length === 0 ? (
        <p className="text-text-secondary">{t("history.empty")}</p>
      ) : (
        <ul className="flex flex-col">
          {entries.map((entry) => (
            <li key={entry.id}>
              <button
                type="button"
                className="hover:bg-text/6 grid w-full grid-cols-[minmax(0,2fr)_minmax(0,3fr)_auto] items-baseline gap-4 rounded-(--radius-control) px-3 py-2 text-left"
                onClick={() => void commands.openTab(entry.url, true)}
              >
                <span className="truncate font-medium">{entry.title || entry.url}</span>
                <span className="text-text-secondary truncate">{entry.url}</span>
                <time className="text-text-secondary" dateTime={new Date(entry.visitedAt).toISOString()}>
                  {new Date(entry.visitedAt).toLocaleString()}
                </time>
              </button>
            </li>
          ))}
        </ul>
      )}
    </InternalPage>
  );
}
