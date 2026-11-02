import { events, type FindResult, type Tab, type TabId } from "@bindings";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";

/** The longest query the bar sends; Rust refuses longer. */
export const FIND_QUERY_MAX = 1000;

/** The find bar open in this window, and how many times it was asked for. */
export interface OpenFind {
  tab: TabId;
  /** Counts Ctrl+F presses, so another one selects the field again. */
  request: number;
}

/**
 * Which tab the window's find bar is open for, if any.
 *
 * Ctrl+F opens it for the focused tab. It closes when another tab takes focus
 * and when the page navigates, and it keeps nothing of its search once closed.
 * [Find, print and save § Find in page](../../../docs/specs/find-print-save.md#find-in-page).
 *
 * @param tab - The focused tab.
 */
export function useFindBar(tab: Tab | null): { open: OpenFind | null; close: () => void } {
  const [open, setOpen] = useState<OpenFind | null>(null);
  const tabId = tab?.id;
  const visit = tab ? tab.history.entries[tab.history.index] : undefined;
  const page = tab ? `${tab.id}:${tab.history.index}:${visit?.url ?? ""}:${tab.presence.status}` : "";

  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;
    const listen = async () => {
      // This window's own requests only, as for every event sent to one window.
      const stops = await Promise.all([
        events.findRequested(getCurrentWebview()).listen(({ payload }) => {
          setOpen((current) => ({ tab: payload, request: (current?.request ?? 0) + 1 }));
        }),
        // A reload keeps the address, so only Rust can say the page changed.
        events.findEnded(getCurrentWebview()).listen(({ payload }) => {
          setOpen((current) => (current?.tab === payload ? null : current));
        }),
      ]);
      const stop = () => stops.forEach((each) => each());
      if (isMounted) {
        unlisten = stop;
      } else {
        stop();
      }
    };
    void listen();
    return () => {
      isMounted = false;
      unlisten?.();
    };
  }, []);

  // Another tab focused, or a new page in this one, ends the search, and
  // coming back does not reopen it. Adjusted while rendering, as React
  // advises for state that follows a prop.
  const [shownPage, setShownPage] = useState(page);
  if (shownPage !== page) {
    setShownPage(page);
    setOpen(null);
  }

  return { open: open !== null && open.tab === tabId ? open : null, close: () => setOpen(null) };
}

interface FindBarProps {
  tab: TabId;
  /** Changes on every Ctrl+F, which selects the field again. */
  request: number;
  onClose: () => void;
}

/**
 * The find bar: a field, the match count, previous and next, and close.
 *
 * Searches as it is typed in. Enter moves to the next match and Shift+Enter
 * to the previous one; Escape closes. Closing, for any reason, clears the
 * page's highlights.
 */
export function FindBar({ tab, request, onClose }: FindBarProps) {
  const rootRef = useRef<HTMLElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<Omit<FindResult, "tab"> | null>(null);
  const isStopped = useRef(false);
  useOverlay(rootRef, true);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [request]);

  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | null = null;
    const listen = async () => {
      const stop = await events.findResult(getCurrentWebview()).listen(({ payload }) => {
        if (payload.tab === tab) {
          setResult({ matches: payload.matches, active: payload.active });
        }
      });
      if (isMounted) {
        unlisten = stop;
      } else {
        stop();
      }
    };
    void listen();
    return () => {
      isMounted = false;
      unlisten?.();
      // Closed by focus moving elsewhere, which keeps the keyboard there.
      if (!isStopped.current) {
        void commands.stopFind(tab, false);
      }
    };
  }, [tab]);

  // Closed by the user, who gets the page's keyboard back.
  const close = () => {
    isStopped.current = true;
    void commands.stopFind(tab, true);
    onClose();
  };

  const search = (next: string) => {
    setQuery(next);
    setResult(null);
    if (next === "") {
      void commands.stopFind(tab, false);
    } else {
      void commands.findInPage(tab, next, "first");
    }
  };
  const move = (direction: "next" | "previous") => {
    if (query !== "") {
      void commands.findInPage(tab, query, direction);
    }
  };

  // On the field and every button, so Esc closes the bar wherever its focus is.
  const closeOnEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  };

  const count =
    query === "" || result === null
      ? ""
      : result.matches === 0
        ? t("find.none")
        : t("find.count")
            .replace("{active}", () => String(result.active ?? 0))
            .replace("{matches}", () => String(result.matches));

  return (
    <search
      ref={rootRef}
      aria-label={t("find.label")}
      className="haku-bar bg-chrome-raised border-text/15 text-text absolute top-2 right-2 z-10 flex w-80 max-w-[calc(100%-1rem)] items-center gap-1 rounded-(--radius-surface) border py-1 pr-1 pl-2.5"
    >
      <input
        ref={inputRef}
        type="text"
        value={query}
        maxLength={FIND_QUERY_MAX}
        onChange={(event) => search(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            move(event.shiftKey ? "previous" : "next");
          } else {
            closeOnEscape(event);
          }
        }}
        spellCheck={false}
        autoComplete="off"
        aria-label={t("find.label")}
        placeholder={t("find.label")}
        className="placeholder:text-text-secondary min-w-0 flex-1 bg-transparent outline-none select-text"
      />
      <span className="text-text-secondary shrink-0 text-sm tabular-nums" aria-live="polite">
        {count}
      </span>
      <FindButton
        icon="ic-chevron-up"
        label={t("find.previous")}
        onClick={() => move("previous")}
        onKeyDown={closeOnEscape}
      />
      <FindButton
        icon="ic-chevron-down"
        label={t("find.next")}
        onClick={() => move("next")}
        onKeyDown={closeOnEscape}
      />
      <FindButton icon="ic-x" label={t("find.close")} onClick={close} onKeyDown={closeOnEscape} />
    </search>
  );
}

function FindButton({
  icon,
  label,
  onClick,
  onKeyDown,
}: {
  icon: string;
  label: string;
  onClick: () => void;
  onKeyDown: (event: KeyboardEvent) => void;
}) {
  return (
    <button
      type="button"
      className="btn icon ghost shrink-0"
      aria-label={label}
      title={label}
      onClick={onClick}
      onKeyDown={onKeyDown}
    >
      <i className={cx(icon, "ic-sm")} aria-hidden="true" />
    </button>
  );
}
