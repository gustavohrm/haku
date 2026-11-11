import { type TabId } from "@bindings";
import { Favicon } from "@features/bookmarks/favicon";
import { openMenu } from "@features/context-menu/store";
import { commands } from "@ipc/commands";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useEffect, useRef, useState, type MouseEvent } from "react";

import { type DayGroup, type VisitRow } from "./group";
import { counted, countVisits, siteFilter, siteRuleFor } from "./history-actions";
import { addDays, startOfDay, TRANSITIONS } from "./page-filter";
import { rowMenu } from "./row-menu";

interface VisitListProps {
  tabId: TabId;
  groups: DayGroup[];
  /** When the list was asked for, which day headings are named from. */
  now: number;
  isPrivate: boolean;
  /** Selected visit ids. */
  selected: ReadonlySet<number>;
  onSelect: (ids: number[], selected: boolean) => void;
  showSite: (host: string) => void;
  more: boolean;
  loading: boolean;
  loadMore: () => void;
}

/** Today, Yesterday, or the date, for a day heading. */
export function dayLabel(day: number, now: number): string {
  const today = startOfDay(now);
  if (day === today) {
    return t("history.day.today");
  }
  if (day === addDays(today, -1)) {
    return t("history.day.yesterday");
  }
  return new Date(day).toLocaleDateString(undefined, {
    weekday: "long",
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

const timeOf = (time: number) => new Date(time).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

/**
 * Visits under day headings, a row per page per day, loading more as the end
 * of the list comes into view, or from its button where nothing reports that.
 */
export function VisitList({ groups, now, more, loading, loadMore, ...rowProps }: VisitListProps) {
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const target = end.current;
    if (!more || target === null || typeof IntersectionObserver === "undefined") {
      return;
    }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        loadMore();
      }
    });
    observer.observe(target);
    return () => observer.disconnect();
  }, [more, loadMore]);

  return (
    <div className="flex flex-col gap-6">
      {groups.map((group) => (
        <section key={group.day} aria-label={dayLabel(group.day, now)}>
          <h2 className="text-text-secondary mb-1 px-3 text-sm font-medium">{dayLabel(group.day, now)}</h2>
          <ul className="flex flex-col">
            {group.rows.map((row) => (
              <Row key={row.key} row={row} {...rowProps} />
            ))}
          </ul>
        </section>
      ))}
      {more && (
        <div ref={end} className="flex justify-center">
          <button type="button" className="btn soft edgeless" disabled={loading} onClick={loadMore}>
            {t("history.showMore")}
          </button>
        </div>
      )}
    </div>
  );
}

type RowProps = Omit<VisitListProps, "groups" | "now" | "more" | "loading" | "loadMore"> & { row: VisitRow };

function Row({ row, tabId, isPrivate, selected, onSelect, showSite }: RowProps) {
  const [open, setOpen] = useState(false);
  const ids = row.visits.map((visit) => visit.id);
  const checked = ids.every((id) => selected.has(id));
  // Once anything is selected, every row shows its checkbox; until then a
  // row shows it, and its actions, only on hover or focus.
  const reveal = "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100";
  const selecting = selected.size > 0;
  const name = row.title || row.url;
  const newest = row.visits[0]?.visitedAt ?? 0;

  const follow = (event: MouseEvent) => {
    if (event.button === 1 || event.ctrlKey || event.metaKey) {
      void commands.openTab(row.url, false, "other");
    } else if (event.button === 0) {
      void commands.navigateTab(tabId, row.url, "other");
    }
  };
  // Asks which rule decides the page, and how much of its site is recorded,
  // as the menu opens, so it says both as they are now rather than as they
  // were when the list loaded.
  const showMenu = async (at: { x: number; y: number }) => {
    const hasSite = row.host !== "";
    const [siteRule, siteVisits] = await Promise.all([
      isPrivate || !hasSite ? null : siteRuleFor(row.url, row.host),
      hasSite ? countVisits(siteFilter(row.host)) : null,
    ]);
    openMenu({ at, label: name, items: rowMenu(row, { isPrivate, siteRule, siteVisits, showSite }) });
  };

  return (
    // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Right-click opens the row's menu; its actions button opens the same from the keyboard.
    <li
      className="group hover:bg-text/6 rounded-(--radius-control)"
      onContextMenu={(event) => {
        event.preventDefault();
        void showMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      <div className="flex items-center gap-3 px-3 py-1.5">
        {/* Hidden from a wrapper, which the checkbox's own styles cannot outrank. */}
        <span className={cx("flex", !selecting && reveal)}>
          <input
            type="checkbox"
            className="checkbox"
            aria-label={t("history.row.select").replace("{page}", () => name)}
            checked={checked}
            onChange={(event) => onSelect(ids, event.target.checked)}
          />
        </span>
        <Favicon key={row.url} url={isPrivate ? null : row.url} />
        <button
          type="button"
          className="flex min-w-0 flex-1 items-baseline gap-3 text-left"
          title={row.url}
          onClick={follow}
          onAuxClick={(event) => {
            if (event.button === 1) {
              event.preventDefault();
              follow(event);
            }
          }}
        >
          <span className="truncate font-medium">{name}</span>
          {row.host !== "" && <span className="text-text-secondary shrink-0 truncate text-sm">{row.host}</span>}
        </button>
        {row.visits.length > 1 && (
          <button
            type="button"
            className="btn ghost text-text-secondary text-sm"
            aria-expanded={open}
            onClick={() => setOpen((current) => !current)}
          >
            {counted(row.visits.length, "history.row.visits.one", "history.row.visits.many")}
            <i className={open ? "ic-chevron-up ic-sm" : "ic-chevron-down ic-sm"} aria-hidden="true" />
          </button>
        )}
        <time className="text-text-secondary shrink-0 text-sm tabular-nums" dateTime={new Date(newest).toISOString()}>
          {timeOf(newest)}
        </time>
        <button
          type="button"
          className={cx("btn icon ghost size-7", reveal)}
          aria-label={t("history.row.actions").replace("{page}", () => name)}
          title={t("history.row.actions").replace("{page}", () => name)}
          aria-haspopup="menu"
          onClick={(event) => {
            const box = event.currentTarget.getBoundingClientRect();
            void showMenu({ x: box.left, y: box.bottom });
          }}
        >
          <i className="ic-ellipsis ic-sm" aria-hidden="true" />
        </button>
      </div>
      {open && (
        <ul className="text-text-secondary flex flex-col gap-0.5 pb-2 pl-17 text-sm">
          {row.visits.map((visit) => (
            <li key={visit.id} className="flex gap-3">
              <time className="tabular-nums" dateTime={new Date(visit.visitedAt).toISOString()}>
                {timeOf(visit.visitedAt)}
              </time>
              <span>{t(TRANSITIONS[visit.transition])}</span>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}
