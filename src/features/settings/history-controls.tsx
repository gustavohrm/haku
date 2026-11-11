import {
  type HakuError,
  type HistoryRetention,
  type HistoryRule,
  type HistoryRuleAction,
  type UrlPattern,
  type UrlPatternKind,
} from "@bindings";
import { reportUnexpected } from "@features/feedback/feedback";
import { filterOf, retentionCutoff, shortens } from "@features/history/filter";
import { commands } from "@ipc/commands";
import { describeError, unwrap } from "@ipc/result";
import { tryUpdateSettings } from "@ipc/store";
import { cx } from "@shared/class-names";
import { t, type TranslationKey } from "@shared/i18n";
import { useId, useRef, useState } from "react";

import { guessPattern, moveItem } from "./rule-pattern";
import { type ControlProps } from "./types";

const SELECT_CLASS = "select soft edgeless";

/** Whether browsing history is recorded at all. */
export function HistoryEnabledControl({ id, describedBy, settings, update }: ControlProps) {
  return (
    <input
      id={id}
      type="checkbox"
      className="switch"
      aria-describedby={describedBy}
      checked={settings.historyEnabled}
      onChange={(event) => void update({ historyEnabled: event.target.checked })}
    />
  );
}

const RETENTIONS: Record<HistoryRetention, TranslationKey> = {
  forever: "settings.historyRetention.forever",
  year: "settings.historyRetention.year",
  days90: "settings.historyRetention.days90",
  days30: "settings.historyRetention.days30",
};

function isRetention(value: string): value is HistoryRetention {
  return value in RETENTIONS;
}

/**
 * How long visits are kept. Keeping them for less time deletes the older ones
 * at once, so that asks first, saying how many go.
 */
export function HistoryRetentionControl({ id, describedBy, settings, update }: ControlProps) {
  const [pending, setPending] = useState<{
    retention: HistoryRetention;
    count: number;
  } | null>(null);

  const choose = async (retention: HistoryRetention) => {
    setPending(null);
    const cutoff = retentionCutoff(retention, Date.now());
    if (cutoff === null || !shortens(settings.historyRetention, retention)) {
      await update({ historyRetention: retention });
      return;
    }
    try {
      const count = await unwrap(commands.countHistory(filterOf({ to: cutoff })));
      if (count === 0) {
        await update({ historyRetention: retention });
      } else {
        setPending({ retention, count });
      }
    } catch (error) {
      reportUnexpected(error);
    }
  };

  return (
    <div className="flex flex-col items-end gap-2">
      <select
        id={id}
        className={`${SELECT_CLASS} w-44`}
        aria-describedby={describedBy}
        value={pending?.retention ?? settings.historyRetention}
        onChange={(event) => {
          const retention = event.target.value;
          if (isRetention(retention)) {
            void choose(retention);
          }
        }}
      >
        {Object.entries(RETENTIONS).map(([value, label]) => (
          <option key={value} value={value}>
            {t(label)}
          </option>
        ))}
      </select>
      {pending && (
        <div role="alert" className="flex max-w-80 flex-col items-end gap-2 text-right text-sm">
          <p>{counted(pending.count, "settings.historyRetention.confirm")}</p>
          <div className="flex gap-2">
            <button type="button" className="btn ghost" onClick={() => setPending(null)}>
              {t("dialog.cancel")}
            </button>
            <button
              type="button"
              className="btn destructive"
              onClick={() => {
                setPending(null);
                void update({ historyRetention: pending.retention });
              }}
            >
              {t("settings.historyRetention.delete")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/** `key.one` for one, otherwise `key.many` with `{count}` filled in. */
function counted(count: number, key: "settings.historyRetention.confirm" | "settings.historyRules.offer"): string {
  return count === 1 ? t(`${key}.one`) : t(`${key}.many`).replace("{count}", count.toLocaleString());
}

const KINDS: Record<UrlPatternKind, TranslationKey> = {
  site: "settings.historyRules.kind.site",
  page: "settings.historyRules.kind.page",
  wildcard: "settings.historyRules.kind.wildcard",
  regex: "settings.historyRules.kind.regex",
};

const ACTIONS: Record<HistoryRuleAction, TranslationKey> = {
  never: "settings.historyRules.action.never",
  addressOnly: "settings.historyRules.action.addressOnly",
  always: "settings.historyRules.action.always",
};

function isKind(value: string): value is UrlPatternKind {
  return value in KINDS;
}

function isAction(value: string): value is HistoryRuleAction {
  return value in ACTIONS;
}

function samePattern(a: UrlPattern, b: UrlPattern): boolean {
  return a.kind === b.kind && a.value === b.value;
}

/** A rule's place in the list while it is dragged: where it started, and where it would land. */
interface Drag {
  from: number;
  to: number;
  pointerId: number;
}

/**
 * The history rules in the order they are checked, each a line whose action
 * is changed in place, dragged by its handle to reorder, or moved with the
 * arrow keys on the handle; then the field that adds one at the top.
 * [History § Rules](../../../docs/specs/history.md#rules).
 */
export function HistoryRulesControl({ labelId, describedBy, settings, update }: ControlProps) {
  const rules = settings.historyRules;
  const [drag, setDrag] = useState<Drag | null>(null);
  const handles = useRef<(HTMLButtonElement | null)[]>([]);

  const move = async (from: number, to: number) => {
    if (from === to) {
      return;
    }
    await update((current) => ({ ...current, historyRules: moveItem(current.historyRules, from, to) }));
    handles.current[to]?.focus();
  };

  const setAction = (rule: HistoryRule, action: HistoryRuleAction) =>
    void update((current) => ({
      ...current,
      historyRules: current.historyRules.map((other) =>
        samePattern(other.pattern, rule.pattern) ? { ...other, action } : other,
      ),
    }));

  const remove = (rule: HistoryRule) =>
    void update((current) => ({
      ...current,
      historyRules: current.historyRules.filter((other) => !samePattern(other.pattern, rule.pattern)),
    }));

  const dropAt = (x: number, y: number): number | null => {
    const row = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-rule-index]");
    return row ? Number(row.dataset.ruleIndex) : null;
  };

  return (
    <div className="flex flex-col gap-2">
      {rules.length === 0 ? (
        <p className="text-text-secondary text-sm">{t("settings.historyRules.empty")}</p>
      ) : (
        <ol className="flex flex-col" aria-labelledby={labelId} aria-describedby={describedBy}>
          {rules.map((rule, index) => {
            const id = `history-rule-${index}`;
            const isTarget = drag !== null && drag.to === index && drag.from !== index;
            return (
              <li
                key={`${rule.pattern.kind} ${rule.pattern.value}`}
                data-rule-index={index}
                className={cx(
                  "group hover:bg-text/5 flex min-h-9 items-center gap-2 rounded-(--radius-control) pr-1",
                  drag?.from === index && "bg-text/5",
                  isTarget &&
                    (drag.to < drag.from
                      ? "shadow-[inset_0_2px_0_var(--color-text)]"
                      : "shadow-[inset_0_-2px_0_var(--color-text)]"),
                )}
              >
                <button
                  ref={(element) => {
                    handles.current[index] = element;
                  }}
                  type="button"
                  className="btn icon ghost cursor-grab touch-none opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"
                  aria-label={t("settings.historyRules.reorder")}
                  aria-describedby={id}
                  title={t("settings.historyRules.reorder")}
                  onKeyDown={(event) => {
                    const by = event.key === "ArrowUp" ? -1 : event.key === "ArrowDown" ? 1 : 0;
                    if (by !== 0) {
                      event.preventDefault();
                      void move(index, Math.min(Math.max(index + by, 0), rules.length - 1));
                    }
                  }}
                  onPointerDown={(event) => {
                    event.currentTarget.setPointerCapture(event.pointerId);
                    setDrag({ from: index, to: index, pointerId: event.pointerId });
                  }}
                  onPointerMove={(event) => {
                    if (drag?.pointerId !== event.pointerId) {
                      return;
                    }
                    const to = dropAt(event.clientX, event.clientY);
                    if (to !== null && to !== drag.to) {
                      setDrag({ ...drag, to });
                    }
                  }}
                  onPointerUp={() => {
                    if (drag) {
                      void move(drag.from, drag.to);
                    }
                    setDrag(null);
                  }}
                  onPointerCancel={() => setDrag(null)}
                >
                  <i className="ic-grip-vertical ic-sm" aria-hidden="true" />
                </button>
                {/* Each control is described by the rule, so it reads as "Remove rule, example.com, Site". */}
                <span id={id} className="flex min-w-0 flex-1 items-baseline gap-2">
                  <span className="truncate font-mono text-sm" title={rule.pattern.value}>
                    {rule.pattern.value}
                  </span>
                  <span className="text-text-secondary shrink-0 text-xs">{t(KINDS[rule.pattern.kind])}</span>
                </span>
                <select
                  className="select soft edgeless w-40 shrink-0"
                  aria-label={t("settings.historyRules.action")}
                  aria-describedby={id}
                  value={rule.action}
                  onChange={(event) => {
                    const next = event.target.value;
                    if (isAction(next)) {
                      setAction(rule, next);
                    }
                  }}
                >
                  {Object.entries(ACTIONS).map(([option, label]) => (
                    <option key={option} value={option}>
                      {t(label)}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  className="btn icon ghost opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"
                  aria-label={t("settings.historyRules.remove")}
                  aria-describedby={id}
                  title={t("settings.historyRules.remove")}
                  onClick={() => remove(rule)}
                >
                  <i className="ic-trash ic-sm" aria-hidden="true" />
                </button>
              </li>
            );
          })}
        </ol>
      )}
      <AddRuleForm />
    </div>
  );
}

/** A rule waiting on the choice to delete the visits it matches. */
interface Offer {
  rule: HistoryRule;
  count: number;
}

/**
 * One field that adds a rule at the top, so it decides before every older
 * one. The pattern's kind is guessed from what is typed and shown as a tag
 * that can be changed; the action is chosen beside it. Both appear only once
 * something is typed. A `never` rule that matches recorded visits first
 * offers to delete them, in place; nothing is deleted without that choice. An
 * invalid pattern is shown under the field.
 */
function AddRuleForm() {
  const valueId = useId();
  const errorId = useId();
  const [value, setValue] = useState("");
  // A kind chosen by hand, which the guess no longer replaces.
  const [chosenKind, setChosenKind] = useState<UrlPatternKind | null>(null);
  const [action, setAction] = useState<HistoryRuleAction>("never");
  const [error, setError] = useState<string | null>(null);
  const [offer, setOffer] = useState<Offer | null>(null);

  const guessed = guessPattern(value);
  const pattern: UrlPattern =
    chosenKind === null || chosenKind === guessed.kind ? guessed : { kind: chosenKind, value: value.trim() };
  const typing = value.trim() !== "";

  const fail = (failure: HakuError) =>
    setError(failure.kind === "InvalidPattern" ? failure.message : describeError(failure));

  const save = async (rule: HistoryRule, deleteMatching: boolean) => {
    setOffer(null);
    const result = await tryUpdateSettings((current) => ({
      ...current,
      historyRules: [rule, ...current.historyRules.filter((other) => !samePattern(other.pattern, rule.pattern))],
    }));
    if (result === null) {
      return;
    }
    if (result.status === "error") {
      fail(result.error);
      return;
    }
    setValue("");
    setChosenKind(null);
    if (deleteMatching) {
      try {
        await unwrap(commands.deleteHistory(filterOf({ include: [rule.pattern] })));
      } catch (failure) {
        reportUnexpected(failure);
      }
    }
  };

  const submit = async () => {
    setError(null);
    const rule: HistoryRule = { pattern, action };
    if (action !== "never") {
      await save(rule, false);
      return;
    }
    // Counting compiles the pattern, so an invalid one is refused here, before anything is stored.
    const counted = await commands.countHistory(filterOf({ include: [rule.pattern] }));
    if (counted.status === "error") {
      fail(counted.error);
    } else if (counted.data === 0) {
      await save(rule, false);
    } else {
      setOffer({ rule, count: counted.data });
    }
  };

  return (
    <form
      className="flex flex-col gap-2"
      aria-label={t("settings.historyRules.add")}
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className="input-group soft edgeless min-w-56 flex-1">
          <i className="ic-plus ic-sm text-text-secondary shrink-0" aria-hidden="true" />
          <label htmlFor={valueId} className="sr-only">
            {t("settings.historyRules.pattern")}
          </label>
          <input
            id={valueId}
            type="text"
            className="ipt font-mono"
            placeholder={t("settings.historyRules.placeholder")}
            spellCheck={false}
            autoComplete="off"
            aria-invalid={error !== null}
            aria-describedby={error === null ? undefined : errorId}
            value={value}
            onChange={(event) => {
              setValue(event.target.value);
              setError(null);
              setOffer(null);
              if (event.target.value.trim() === "") {
                setChosenKind(null);
              }
            }}
          />
          {typing && (
            <select
              className="text-text-secondary shrink-0 cursor-pointer rounded-(--radius-small) border-0 bg-transparent text-xs"
              aria-label={t("settings.historyRules.kind")}
              value={pattern.kind}
              onChange={(event) => {
                const next = event.target.value;
                if (isKind(next)) {
                  setChosenKind(next);
                  setError(null);
                }
              }}
            >
              {Object.entries(KINDS).map(([option, label]) => (
                <option key={option} value={option}>
                  {t(label)}
                </option>
              ))}
            </select>
          )}
        </span>
        {typing && (
          <>
            <fieldset className="m-0 flex gap-0.5 border-0 p-0" aria-label={t("settings.historyRules.action")}>
              {Object.entries(ACTIONS).map(([option, label]) => (
                <label key={option} className={cx("btn sm", option === action ? "soft" : "ghost")}>
                  <input
                    type="radio"
                    className="sr-only"
                    name={`${valueId}-action`}
                    value={option}
                    checked={option === action}
                    onChange={() => {
                      if (isAction(option)) {
                        setAction(option);
                      }
                    }}
                  />
                  {t(label)}
                </label>
              ))}
            </fieldset>
            <button type="submit" className="btn" disabled={offer !== null}>
              {t("settings.historyRules.add")}
            </button>
          </>
        )}
      </div>
      {error !== null && (
        <p id={errorId} role="alert" className="text-destructive text-sm">
          {error}
        </p>
      )}
      {offer && (
        <div role="alert" className="bg-text/5 flex flex-col gap-2 rounded-(--radius-control) p-2 text-sm">
          <p>{counted(offer.count, "settings.historyRules.offer")}</p>
          <div className="flex flex-wrap justify-end gap-2">
            <button type="button" className="btn ghost" onClick={() => setOffer(null)}>
              {t("dialog.cancel")}
            </button>
            <button type="button" className="btn soft edgeless" onClick={() => void save(offer.rule, false)}>
              {t("settings.historyRules.offer.keep")}
            </button>
            <button type="button" className="btn destructive" onClick={() => void save(offer.rule, true)}>
              {offer.count === 1
                ? t("settings.historyRules.offer.delete.one")
                : t("settings.historyRules.offer.delete.many").replace("{count}", offer.count.toLocaleString())}
            </button>
          </div>
        </div>
      )}
    </form>
  );
}
