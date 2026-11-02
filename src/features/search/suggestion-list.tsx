import { type Suggestion } from "@bindings";
import { useOverlay } from "@features/overlays/use-overlay";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useRef } from "react";

interface SuggestionListProps {
  id: string;
  rows: Suggestion[];
  selected: number;
  /** The id of the row at `index`, which the input names as active. */
  rowId: (index: number) => string;
  onChoose: (index: number) => void;
  className?: string;
}

/**
 * The rows under the address field. Drawn over the page, so registered as an
 * overlay. Unframed: the caller edges it, as a list of its own or as part of
 * the floating card. A row takes no focus: the input keeps it, and names the selected
 * row as its active descendant.
 * [Address bar § Suggestions](../../../docs/specs/address-bar.md#suggestions).
 */
export function SuggestionList({ id, rows, selected, rowId, onChoose, className }: SuggestionListProps) {
  const listRef = useRef<HTMLDivElement>(null);
  useOverlay(listRef, rows.length > 0);
  if (rows.length === 0) {
    return null;
  }
  return (
    <div
      ref={listRef}
      id={id}
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- A combobox's popup: a select cannot hang under a text input while the input keeps focus.
      role="listbox"
      aria-label={t("toolbar.suggestions")}
      className={cx("bg-chrome-raised z-30 flex flex-col overflow-hidden py-1", className)}
    >
      {rows.map((row, index) => {
        const { icon, title, detail, bookmarked } = describe(row);
        return (
          // oxlint-disable-next-line jsx-a11y/click-events-have-key-events -- The input keeps focus and takes the keys, naming this row its active descendant.
          <div
            key={rowKey(row, index)}
            id={rowId(index)}
            // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- An option of the combobox above, which a select's options cannot be.
            role="option"
            tabIndex={-1}
            aria-selected={index === selected}
            className={cx(
              "flex h-(--control-height) cursor-default items-center gap-2 px-2.5",
              index === selected && "bg-text/10",
            )}
            // Pressing a row would otherwise blur the input, which closes the list.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => onChoose(index)}
          >
            <i className={cx(icon, "ic-sm text-text-secondary shrink-0")} aria-hidden="true" />
            <span className="text-text min-w-0 shrink truncate">{title}</span>
            {bookmarked && <span className="sr-only">{t("toolbar.suggest.bookmarked")}</span>}
            {detail !== "" && <span className="text-text-secondary min-w-0 flex-1 truncate text-sm">{detail}</span>}
          </div>
        );
      })}
    </div>
  );
}

interface Described {
  icon: string;
  title: string;
  detail: string;
  bookmarked: boolean;
}

function describe(row: Suggestion): Described {
  switch (row.kind) {
    case "search": {
      return {
        icon: "ic-search",
        title: row.query,
        detail: t("toolbar.suggest.search").replace("{engine}", () => row.engine),
        bookmarked: false,
      };
    }
    case "visit": {
      return { icon: "ic-globe", title: row.url, detail: t("toolbar.suggest.visit"), bookmarked: false };
    }
    case "tab": {
      return {
        icon: "ic-arrow-right",
        title: row.title || row.url,
        detail: t("toolbar.suggest.switch"),
        bookmarked: false,
      };
    }
    case "page": {
      return {
        icon: row.bookmarked ? "ic-bookmark" : "ic-history",
        title: row.title || row.url,
        detail: row.title ? row.url : "",
        bookmarked: row.bookmarked,
      };
    }
    case "engine": {
      return {
        icon: "ic-search",
        title: row.keyword,
        detail: t("toolbar.suggest.engine").replace("{engine}", () => row.name),
        bookmarked: false,
      };
    }
  }
}

function rowKey(row: Suggestion, index: number): string {
  switch (row.kind) {
    case "tab": {
      return `tab-${row.tab}`;
    }
    case "page": {
      return `page-${row.url}`;
    }
    case "engine": {
      return `engine-${row.keyword}`;
    }
    default: {
      return `enter-${index}`;
    }
  }
}
