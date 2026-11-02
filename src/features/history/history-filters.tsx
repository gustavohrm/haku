import { type Transition, type UrlPattern } from "@bindings";
import { openMenu } from "@features/context-menu/store";
import { MENU_GAP } from "@features/overlays/use-menu-dismiss";
import { t } from "@shared/i18n";
import { useId, useState } from "react";

import {
  chipsOf,
  describePattern,
  PATTERN_KINDS,
  TRANSITIONS,
  WHENS,
  withPattern,
  type PageFilter,
  type When,
} from "./page-filter";

interface FiltersProps {
  filter: PageFilter;
  onChange: (next: PageFilter) => void;
}

function isWhen(value: string): value is When {
  return value in WHENS;
}

function isPatternKind(value: string): value is keyof typeof PATTERN_KINDS {
  return value in PATTERN_KINDS;
}

function describeWhen(filter: PageFilter): string {
  if (filter.when !== "custom") {
    return t(WHENS[filter.when]);
  }
  const format = (day: string) => (day === "" ? "…" : new Date(`${day}T00:00`).toLocaleDateString());
  return `${format(filter.fromDay)} – ${format(filter.toDay)}`;
}

/**
 * The search field and the filters beside it, with each active filter as a
 * removable chip. [History § The history page](../../../docs/specs/history.md#the-history-page).
 */
export function HistoryFilters({ filter, onChange }: FiltersProps) {
  const searchId = useId();
  const whenId = useId();
  const [adding, setAdding] = useState(false);

  const toggleTransition = (transition: Transition) =>
    onChange({
      ...filter,
      transitions: filter.transitions.includes(transition)
        ? filter.transitions.filter((other) => other !== transition)
        : [...filter.transitions, transition],
    });

  const chips = chipsOf(filter, {
    when: describeWhen,
    pattern: describePattern,
    transition: (transition) => t(TRANSITIONS[transition]),
  });

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <label htmlFor={searchId} className="sr-only">
          {t("history.search")}
        </label>
        <input
          id={searchId}
          type="search"
          className="ipt soft edgeless min-w-48 flex-1"
          placeholder={t("history.search")}
          spellCheck={false}
          autoComplete="off"
          value={filter.text}
          onChange={(event) => onChange({ ...filter, text: event.target.value })}
        />
        <label htmlFor={whenId} className="sr-only">
          {t("history.when")}
        </label>
        <select
          id={whenId}
          className="select soft edgeless"
          value={filter.when}
          onChange={(event) => {
            const when = event.target.value;
            if (isWhen(when)) {
              onChange({ ...filter, when });
            }
          }}
        >
          {Object.entries(WHENS).map(([value, label]) => (
            <option key={value} value={value}>
              {t(label)}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="btn soft edgeless"
          aria-haspopup="menu"
          onClick={(event) => {
            const box = event.currentTarget.getBoundingClientRect();
            openMenu({
              at: { x: box.left, y: box.bottom + MENU_GAP },
              label: t("history.reachedBy"),
              items: Object.entries(TRANSITIONS).map(([transition, label]) => ({
                kind: "checkbox" as const,
                id: transition,
                label: t(label),
                checked: filter.transitions.includes(transition as Transition),
                onSelect: () => toggleTransition(transition as Transition),
              })),
            });
          }}
        >
          {t("history.reachedBy")}
          <i className="ic-chevron-down ic-sm" aria-hidden="true" />
        </button>
        <button
          type="button"
          className="btn soft edgeless"
          aria-expanded={adding}
          onClick={() => setAdding((open) => !open)}
        >
          <i className="ic-filter ic-sm" aria-hidden="true" />
          {t("history.addressFilter")}
        </button>
      </div>
      {filter.when === "custom" && (
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <label className="flex items-center gap-2">
            {t("history.when.from")}
            <input
              type="date"
              className="ipt soft edgeless"
              value={filter.fromDay}
              onChange={(event) => onChange({ ...filter, fromDay: event.target.value })}
            />
          </label>
          <label className="flex items-center gap-2">
            {t("history.when.to")}
            <input
              type="date"
              className="ipt soft edgeless"
              value={filter.toDay}
              onChange={(event) => onChange({ ...filter, toDay: event.target.value })}
            />
          </label>
        </div>
      )}
      {adding && (
        <AddressFilterForm
          onAdd={(mode, pattern) => {
            onChange(withPattern(filter, mode, pattern));
            setAdding(false);
          }}
        />
      )}
      {chips.length > 0 && (
        <ul className="flex flex-wrap gap-2" aria-label={t("history.activeFilters")}>
          {chips.map((chip) => (
            <li
              key={chip.id}
              className="bg-text/6 flex max-w-full items-center gap-1 rounded-full py-0.5 pr-1 pl-3 text-sm"
            >
              {chip.kind === "exclude" && <span className="text-text-secondary">{t("history.chip.not")}</span>}
              <span className="truncate" title={chip.label}>
                {chip.label}
              </span>
              <button
                type="button"
                className="btn icon ghost size-6"
                aria-label={t("history.chip.remove").replace("{filter}", () => chip.label)}
                title={t("history.chip.remove").replace("{filter}", () => chip.label)}
                onClick={() => onChange(chip.remove(filter))}
              >
                <i className="ic-x ic-sm" aria-hidden="true" />
              </button>
            </li>
          ))}
          <li>
            <button
              type="button"
              className="btn ghost text-sm"
              onClick={() => onChange({ ...chips.reduce((current, chip) => chip.remove(current), filter) })}
            >
              {t("history.chip.clear")}
            </button>
          </li>
        </ul>
      )}
    </div>
  );
}

/** Adds a site or address pattern to show, or to leave out. */
function AddressFilterForm({ onAdd }: { onAdd: (mode: "include" | "exclude", pattern: UrlPattern) => void }) {
  const [mode, setMode] = useState<"include" | "exclude">("include");
  const [kind, setKind] = useState<keyof typeof PATTERN_KINDS>("site");
  const [value, setValue] = useState("");
  const modeId = useId();
  const kindId = useId();
  const valueId = useId();
  return (
    <form
      className="flex flex-wrap items-center gap-2"
      aria-label={t("history.addressFilter")}
      onSubmit={(event) => {
        event.preventDefault();
        const trimmed = value.trim();
        if (trimmed !== "") {
          onAdd(mode, { kind, value: trimmed });
        }
      }}
    >
      <label htmlFor={modeId} className="sr-only">
        {t("history.addressFilter.mode")}
      </label>
      <select
        id={modeId}
        className="select soft edgeless"
        value={mode}
        onChange={(event) => setMode(event.target.value === "exclude" ? "exclude" : "include")}
      >
        <option value="include">{t("history.addressFilter.include")}</option>
        <option value="exclude">{t("history.addressFilter.exclude")}</option>
      </select>
      <label htmlFor={kindId} className="sr-only">
        {t("history.addressFilter.kind")}
      </label>
      <select
        id={kindId}
        className="select soft edgeless"
        value={kind}
        onChange={(event) => {
          const next = event.target.value;
          if (isPatternKind(next)) {
            setKind(next);
          }
        }}
      >
        {Object.entries(PATTERN_KINDS).map(([option, label]) => (
          <option key={option} value={option}>
            {t(label)}
          </option>
        ))}
      </select>
      <label htmlFor={valueId} className="sr-only">
        {t("history.addressFilter.value")}
      </label>
      <input
        id={valueId}
        type="text"
        className="ipt soft edgeless min-w-40 flex-1 font-mono"
        placeholder={kind === "site" ? "example.com" : kind === "wildcard" ? "*/issues/*" : "/issues/\\d+$"}
        spellCheck={false}
        autoComplete="off"
        value={value}
        onChange={(event) => setValue(event.target.value)}
      />
      <button type="submit" className="btn soft edgeless" disabled={value.trim() === ""}>
        {t("history.addressFilter.add")}
      </button>
    </form>
  );
}
