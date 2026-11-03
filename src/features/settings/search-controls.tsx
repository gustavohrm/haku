import { type ListedSearchEngine, type SearchEngine, type Settings } from "@bindings";
import { useSearchEngines } from "@features/search/use-search-engines";
import { describeError } from "@ipc/result";
import { tryUpdateSettings } from "@ipc/store";
import { t } from "@shared/i18n";
import { useId, useState } from "react";

import { DraftInput } from "./draft-input";
import { type ControlProps, type UpdateSettings } from "./types";

/*
  Search engines: which one searches go to, the list of presets and custom
  engines with their keywords, and the form that adds one.
  [Address bar § Settings page](../../../docs/specs/address-bar.md#settings-page).
*/

/** Whether the address bar asks the search engine for completions. */
export function SearchSuggestionsControl({ id, describedBy, settings, update }: ControlProps) {
  return (
    <input
      id={id}
      type="checkbox"
      className="switch"
      aria-describedby={describedBy}
      checked={settings.searchSuggestions}
      onChange={(event) => void update({ searchSuggestions: event.target.checked })}
    />
  );
}

/** The engine searches go to, from the engines that show. */
export function SearchEngineControl({ id, describedBy, settings, update }: ControlProps) {
  const engines = useSearchEngines();
  const shown = (engines ?? []).filter((listed) => !listed.hidden);
  return (
    <select
      id={id}
      className="select soft edgeless w-44"
      aria-describedby={describedBy}
      value={settings.searchEngine}
      disabled={engines === null}
      onChange={(event) => void update({ searchEngine: event.target.value })}
    >
      {shown.map(({ engine }) => (
        <option key={engine.id} value={engine.id}>
          {engine.name}
        </option>
      ))}
    </select>
  );
}

/** A preset's keyword as stored, or a custom engine's. */
function withKeyword(current: Settings, listed: ListedSearchEngine, keyword: string): Settings {
  if (listed.preset) {
    return { ...current, presetKeywords: { ...current.presetKeywords, [listed.engine.id]: keyword.trim() } };
  }
  return {
    ...current,
    customSearchEngines: current.customSearchEngines.map((engine) =>
      engine.id === listed.engine.id ? { ...engine, keyword: keyword.trim() === "" ? null : keyword.trim() } : engine,
    ),
  };
}

/**
 * The keyword `stored` keeps for an engine after `typed` was sent. A preset
 * keyword equal to the preset's own is not stored, so it is what was typed.
 */
function storedKeyword(stored: Settings, listed: ListedSearchEngine, typed: string): string {
  if (listed.preset) {
    return stored.presetKeywords[listed.engine.id] ?? typed.trim();
  }
  return stored.customSearchEngines.find((engine) => engine.id === listed.engine.id)?.keyword ?? "";
}

/**
 * Every engine with its keyword, which searches it once when typed before a
 * query. Presets are hidden rather than removed, since their addresses are
 * Haku's; custom engines are removed. The default engine can be neither.
 */
export function SearchEnginesControl({ labelId, describedBy, settings, update }: ControlProps) {
  const engines = useSearchEngines();
  if (engines === null) {
    return null;
  }
  return (
    <div className="flex flex-col gap-3">
      <ul className="flex flex-col gap-1" aria-labelledby={labelId} aria-describedby={describedBy}>
        {engines.map((listed) => (
          <EngineRow key={listed.engine.id} listed={listed} settings={settings} update={update} />
        ))}
      </ul>
      <AddEngineForm />
    </div>
  );
}

interface EngineRowProps {
  listed: ListedSearchEngine;
  settings: Settings;
  update: UpdateSettings;
}

function EngineRow({ listed, settings, update }: EngineRowProps) {
  const nameId = useId();
  const keywordId = useId();
  const { engine, preset, hidden } = listed;
  const isDefault = engine.id === settings.searchEngine;

  const toggleHidden = () =>
    void update((current) => ({
      ...current,
      hiddenSearchEngines: hidden
        ? current.hiddenSearchEngines.filter((id) => id !== engine.id)
        : [...current.hiddenSearchEngines, engine.id],
    }));
  const remove = () =>
    void update((current) => ({
      ...current,
      customSearchEngines: current.customSearchEngines.filter((other) => other.id !== engine.id),
    }));

  const action = !preset
    ? { label: t("settings.searchEngines.remove"), icon: "ic-x", run: remove }
    : hidden
      ? { label: t("settings.searchEngines.show"), icon: "ic-eye", run: toggleHidden }
      : { label: t("settings.searchEngines.hide"), icon: "ic-eye-off", run: toggleHidden };

  return (
    <li className="flex items-center gap-2">
      <span id={nameId} className="flex min-w-0 flex-1 items-baseline gap-2">
        <span className={hidden ? "text-text-secondary truncate" : "truncate"}>{engine.name}</span>
        {!preset && (
          <span className="text-text-secondary truncate font-mono text-sm" title={engine.url}>
            {engine.url}
          </span>
        )}
        {isDefault && (
          <span className="text-text-secondary shrink-0 text-sm">{t("settings.searchEngines.default")}</span>
        )}
      </span>
      <label htmlFor={keywordId} className="sr-only">
        {t("settings.searchEngines.keyword").replace("{engine}", () => engine.name)}
      </label>
      <DraftInput
        id={keywordId}
        type="text"
        className="w-24 font-mono"
        aria-describedby={nameId}
        value={engine.keyword ?? ""}
        onCommit={async (next) => {
          const stored = await update((current) => withKeyword(current, listed, next));
          return stored === null ? null : storedKeyword(stored, listed, next);
        }}
      />
      <button
        type="button"
        className="btn icon ghost"
        aria-label={action.label}
        aria-describedby={nameId}
        title={isDefault ? t("settings.searchEngines.isDefault") : action.label}
        disabled={isDefault}
        onClick={action.run}
      >
        <i className={`${action.icon} ic-sm`} aria-hidden="true" />
      </button>
    </li>
  );
}

/**
 * Adds a custom engine at the end. Rust drops one whose address cannot
 * search, and a keyword another engine has, so either is said under the form
 * rather than vanishing.
 */
function AddEngineForm() {
  const nameId = useId();
  const urlId = useId();
  const keywordId = useId();
  const messageId = useId();
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [keyword, setKeyword] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  const submit = async () => {
    setMessage(null);
    const engine: SearchEngine = {
      id: `custom-${crypto.randomUUID()}`,
      name: name.trim(),
      url: url.trim(),
      suggestUrl: null,
      keyword: keyword.trim() === "" ? null : keyword.trim(),
    };
    const result = await tryUpdateSettings((current) => ({
      ...current,
      customSearchEngines: [...current.customSearchEngines, engine],
    }));
    if (result === null) {
      return;
    }
    if (result.status === "error") {
      setMessage(describeError(result.error));
      return;
    }
    const stored = result.data.customSearchEngines.find((other) => other.id === engine.id);
    if (stored === undefined) {
      setMessage(t("settings.searchEngines.invalidUrl"));
      return;
    }
    setName("");
    setUrl("");
    setKeyword("");
    if (engine.keyword !== null && stored.keyword === null) {
      setMessage(t("settings.searchEngines.keywordTaken").replace("{keyword}", () => engine.keyword ?? ""));
    }
  };

  return (
    <form
      className="flex flex-col gap-2"
      aria-label={t("settings.searchEngines.add")}
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <div className="flex flex-wrap gap-2">
        <label htmlFor={nameId} className="sr-only">
          {t("settings.searchEngines.name")}
        </label>
        <input
          id={nameId}
          type="text"
          className="ipt soft edgeless w-36"
          placeholder={t("settings.searchEngines.name")}
          autoComplete="off"
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
        <label htmlFor={urlId} className="sr-only">
          {t("settings.searchEngines.url")}
        </label>
        <input
          id={urlId}
          type="text"
          className="ipt soft edgeless min-w-48 flex-1 font-mono"
          placeholder="https://example.com/search?q=%s"
          spellCheck={false}
          autoComplete="off"
          aria-invalid={message !== null}
          aria-describedby={message === null ? undefined : messageId}
          value={url}
          onChange={(event) => {
            setUrl(event.target.value);
            setMessage(null);
          }}
        />
        <label htmlFor={keywordId} className="sr-only">
          {t("settings.searchEngines.keywordField")}
        </label>
        <input
          id={keywordId}
          type="text"
          className="ipt soft edgeless w-24 font-mono"
          placeholder={t("settings.searchEngines.keywordField")}
          spellCheck={false}
          autoComplete="off"
          value={keyword}
          onChange={(event) => setKeyword(event.target.value)}
        />
        <button type="submit" className="btn soft edgeless" disabled={url.trim() === ""}>
          {t("settings.searchEngines.add")}
        </button>
      </div>
      {message !== null && (
        <p id={messageId} role="alert" className="text-destructive text-sm">
          {message}
        </p>
      )}
    </form>
  );
}
