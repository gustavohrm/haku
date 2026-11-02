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
import { t, type TranslationKey } from "@shared/i18n";
import { useId, useState } from "react";

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

const KINDS: Record<UrlPatternKind, { label: TranslationKey; placeholder: string }> = {
  site: {
    label: "settings.historyRules.kind.site",
    placeholder: "example.com",
  },
  page: {
    label: "settings.historyRules.kind.page",
    placeholder: "https://example.com/page",
  },
  wildcard: {
    label: "settings.historyRules.kind.wildcard",
    placeholder: "*://*.example.com/*",
  },
  regex: {
    label: "settings.historyRules.kind.regex",
    placeholder: "^https://[^/]+/admin",
  },
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

/**
 * The history rules in the order they are checked, each movable and
 * removable, with a form that adds one at the top.
 * [History § Rules](../../../docs/specs/history.md#rules).
 */
export function HistoryRulesControl({ labelId, describedBy, settings, update }: ControlProps) {
  const rules = settings.historyRules;

  const move = (index: number, by: -1 | 1) =>
    void update((current) => {
      const next = [...current.historyRules];
      const [rule] = next.splice(index, 1);
      if (rule) {
        next.splice(index + by, 0, rule);
      }
      return { ...current, historyRules: next };
    });

  const remove = (rule: HistoryRule) =>
    void update((current) => ({
      ...current,
      historyRules: current.historyRules.filter((other) => !samePattern(other.pattern, rule.pattern)),
    }));

  return (
    <div className="flex flex-col gap-3">
      {rules.length === 0 ? (
        <p className="text-text-secondary text-sm">{t("settings.historyRules.empty")}</p>
      ) : (
        <ol className="flex flex-col" aria-labelledby={labelId} aria-describedby={describedBy}>
          {rules.map((rule, index) => {
            const id = `history-rule-${index}`;
            return (
              <li key={`${rule.pattern.kind} ${rule.pattern.value}`} className="flex items-center gap-2">
                {/* Each button is described by the rule, so it reads as "Remove, Site example.com, Never record". */}
                <span id={id} className="flex min-w-0 flex-1 items-baseline gap-2">
                  <span className="text-text-secondary shrink-0 text-sm">{t(KINDS[rule.pattern.kind].label)}</span>
                  <span className="truncate font-mono text-sm" title={rule.pattern.value}>
                    {rule.pattern.value}
                  </span>
                  <span className="text-text-secondary shrink-0 text-sm">{t(ACTIONS[rule.action])}</span>
                </span>
                <button
                  type="button"
                  className="btn icon ghost"
                  aria-label={t("settings.historyRules.up")}
                  aria-describedby={id}
                  title={t("settings.historyRules.up")}
                  disabled={index === 0}
                  onClick={() => move(index, -1)}
                >
                  <i className="ic-arrow-up ic-sm" aria-hidden="true" />
                </button>
                <button
                  type="button"
                  className="btn icon ghost"
                  aria-label={t("settings.historyRules.down")}
                  aria-describedby={id}
                  title={t("settings.historyRules.down")}
                  disabled={index === rules.length - 1}
                  onClick={() => move(index, 1)}
                >
                  <i className="ic-arrow-down ic-sm" aria-hidden="true" />
                </button>
                <button
                  type="button"
                  className="btn icon ghost"
                  aria-label={t("settings.historyRules.remove")}
                  aria-describedby={id}
                  title={t("settings.historyRules.remove")}
                  onClick={() => remove(rule)}
                >
                  <i className="ic-x ic-sm" aria-hidden="true" />
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
 * Adds a rule at the top, so it decides before every older one. A `never`
 * rule that matches recorded visits first offers to delete them; nothing is
 * deleted without that choice. An invalid pattern is shown under the field.
 */
function AddRuleForm() {
  const kindId = useId();
  const valueId = useId();
  const actionId = useId();
  const errorId = useId();
  const [kind, setKind] = useState<UrlPatternKind>("site");
  const [value, setValue] = useState("");
  const [action, setAction] = useState<HistoryRuleAction>("never");
  const [error, setError] = useState<string | null>(null);
  const [offer, setOffer] = useState<Offer | null>(null);

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
    const rule: HistoryRule = {
      pattern: { kind, value: value.trim() },
      action,
    };
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
      <div className="flex flex-wrap gap-2">
        <label htmlFor={kindId} className="sr-only">
          {t("settings.historyRules.kind")}
        </label>
        <select
          id={kindId}
          className={SELECT_CLASS}
          value={kind}
          onChange={(event) => {
            const next = event.target.value;
            if (isKind(next)) {
              setKind(next);
              setError(null);
            }
          }}
        >
          {Object.entries(KINDS).map(([option, { label }]) => (
            <option key={option} value={option}>
              {t(label)}
            </option>
          ))}
        </select>
        <label htmlFor={valueId} className="sr-only">
          {t("settings.historyRules.pattern")}
        </label>
        <input
          id={valueId}
          type="text"
          className="ipt soft edgeless min-w-40 flex-1 font-mono"
          placeholder={KINDS[kind].placeholder}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={error !== null}
          aria-describedby={error === null ? undefined : errorId}
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            setError(null);
            setOffer(null);
          }}
        />
        <label htmlFor={actionId} className="sr-only">
          {t("settings.historyRules.action")}
        </label>
        <select
          id={actionId}
          className={SELECT_CLASS}
          value={action}
          onChange={(event) => {
            const next = event.target.value;
            if (isAction(next)) {
              setAction(next);
            }
          }}
        >
          {Object.entries(ACTIONS).map(([option, label]) => (
            <option key={option} value={option}>
              {t(label)}
            </option>
          ))}
        </select>
        <button type="submit" className="btn soft edgeless" disabled={value.trim() === "" || offer !== null}>
          {t("settings.historyRules.add")}
        </button>
      </div>
      {error !== null && (
        <p id={errorId} role="alert" className="text-destructive text-sm">
          {error}
        </p>
      )}
      {offer && (
        <div role="alert" className="flex flex-col gap-2 text-sm">
          <p>{counted(offer.count, "settings.historyRules.offer")}</p>
          <div className="flex flex-wrap gap-2">
            <button type="button" className="btn destructive" onClick={() => void save(offer.rule, true)}>
              {offer.count === 1
                ? t("settings.historyRules.offer.delete.one")
                : t("settings.historyRules.offer.delete.many").replace("{count}", offer.count.toLocaleString())}
            </button>
            <button type="button" className="btn soft edgeless" onClick={() => void save(offer.rule, false)}>
              {t("settings.historyRules.offer.keep")}
            </button>
            <button type="button" className="btn ghost" onClick={() => setOffer(null)}>
              {t("dialog.cancel")}
            </button>
          </div>
        </div>
      )}
    </form>
  );
}
