import { type ListedSearchEngine, type SearchEngine, type Settings } from "@bindings";
import { useSearchEngines } from "@features/search/use-search-engines";
import { describeError } from "@ipc/result";
import { tryUpdateSettings } from "@ipc/store";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useId, useState } from "react";

import { type ControlProps, type UpdateSettings } from "./types";

/*
  Search engines: one list of every engine, which marks the default, edits an
  engine in place and adds one from a button, as the interface guidelines
  describe for lists.
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

/** What an engine's editor holds: a preset edits only its keyword. */
interface EngineDraft {
  name: string;
  url: string;
  keyword: string;
}

/** Which row is open as a form: an engine's, the new engine's, or none. */
type Editing = { id: string } | "new" | null;

/**
 * Every engine with its keyword, which searches it once when typed before a
 * query. The default is marked, and any other shown engine can become it.
 * Presets are hidden rather than removed, since their addresses are Haku's,
 * and hidden ones fold away; custom engines are removed. The default engine
 * can be neither.
 */
export function SearchEnginesControl({ labelId, describedBy, settings, update }: ControlProps) {
  const engines = useSearchEngines();
  const [editing, setEditing] = useState<Editing>(null);
  const [showHidden, setShowHidden] = useState(false);
  const hiddenId = useId();
  if (engines === null) {
    return null;
  }
  const shown = engines.filter((listed) => !listed.hidden);
  const hidden = engines.filter((listed) => listed.hidden);

  const row = (listed: ListedSearchEngine) =>
    editing !== null && editing !== "new" && editing.id === listed.engine.id ? (
      <li key={listed.engine.id}>
        <EngineEditor listed={listed} onDone={() => setEditing(null)} update={update} />
      </li>
    ) : (
      <EngineRow
        key={listed.engine.id}
        listed={listed}
        isDefault={listed.engine.id === settings.searchEngine}
        update={update}
        onEdit={() => setEditing({ id: listed.engine.id })}
      />
    );

  return (
    <div className="flex flex-col gap-1">
      <ul className="flex flex-col" aria-labelledby={labelId} aria-describedby={describedBy}>
        {shown.map(row)}
      </ul>
      {hidden.length > 0 && (
        <>
          <button
            type="button"
            className="btn ghost justify-start gap-2 self-start"
            aria-expanded={showHidden}
            aria-controls={hiddenId}
            onClick={() => setShowHidden((open) => !open)}
          >
            <i className={cx("ic-sm", showHidden ? "ic-chevron-down" : "ic-chevron-right")} aria-hidden="true" />
            {t("settings.searchEngines.hidden").replace("{count}", String(hidden.length))}
          </button>
          {showHidden && (
            <ul id={hiddenId} className="flex flex-col" aria-label={t("settings.searchEngines.hiddenList")}>
              {hidden.map(row)}
            </ul>
          )}
        </>
      )}
      {editing === "new" ? (
        <EngineEditor listed={null} onDone={() => setEditing(null)} update={update} />
      ) : (
        <button type="button" className="btn ghost justify-start gap-2 self-start" onClick={() => setEditing("new")}>
          <i className="ic-plus ic-sm" aria-hidden="true" />
          {t("settings.searchEngines.add")}
        </button>
      )}
    </div>
  );
}

interface EngineRowProps {
  listed: ListedSearchEngine;
  isDefault: boolean;
  update: UpdateSettings;
  onEdit: () => void;
}

/** One engine as a line: its name, its keyword and whether it is the default, with its actions on hover. */
function EngineRow({ listed, isDefault, update, onEdit }: EngineRowProps) {
  const nameId = useId();
  const { engine, preset, hidden } = listed;

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

  const last = !preset
    ? { label: t("settings.searchEngines.remove"), icon: "ic-trash", run: remove }
    : hidden
      ? { label: t("settings.searchEngines.show"), icon: "ic-eye", run: toggleHidden }
      : { label: t("settings.searchEngines.hide"), icon: "ic-eye-off", run: toggleHidden };

  return (
    <li className="group hover:bg-text/5 flex min-h-9 items-center gap-2 rounded-(--radius-control) px-2">
      <span id={nameId} className="flex min-w-0 flex-1 items-baseline gap-2">
        <span className={cx("truncate", hidden && "text-text-secondary")}>{engine.name}</span>
        {engine.keyword && <span className="text-text-secondary shrink-0 font-mono text-sm">{engine.keyword}</span>}
        {isDefault && (
          <span className="bg-text/10 shrink-0 rounded-full px-2 text-xs">{t("settings.searchEngines.default")}</span>
        )}
      </span>
      <span className="flex shrink-0 items-center gap-0.5 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
        {!isDefault && !hidden && (
          <button
            type="button"
            className="btn ghost sm"
            aria-describedby={nameId}
            onClick={() => void update({ searchEngine: engine.id })}
          >
            {t("settings.searchEngines.makeDefault")}
          </button>
        )}
        <button
          type="button"
          className="btn icon ghost"
          aria-label={t("settings.searchEngines.edit")}
          aria-describedby={nameId}
          title={t("settings.searchEngines.edit")}
          onClick={onEdit}
        >
          <i className="ic-pencil ic-sm" aria-hidden="true" />
        </button>
        {!isDefault && (
          <button
            type="button"
            className="btn icon ghost"
            aria-label={last.label}
            aria-describedby={nameId}
            title={last.label}
            onClick={last.run}
          >
            <i className={`${last.icon} ic-sm`} aria-hidden="true" />
          </button>
        )}
      </span>
    </li>
  );
}

/** A preset's keyword as stored, or a custom engine's whole entry. */
function withEngine(current: Settings, listed: ListedSearchEngine, next: SearchEngine): Settings {
  if (listed.preset) {
    return { ...current, presetKeywords: { ...current.presetKeywords, [listed.engine.id]: next.keyword ?? "" } };
  }
  return {
    ...current,
    customSearchEngines: current.customSearchEngines.map((engine) => (engine.id === next.id ? next : engine)),
  };
}

/** The keyword `stored` kept for `id`, a preset's or a custom engine's. */
function keptKeyword(stored: Settings, listed: ListedSearchEngine | null, id: string, sent: string | null) {
  if (listed?.preset) {
    return stored.presetKeywords[id] ?? sent;
  }
  return stored.customSearchEngines.find((engine) => engine.id === id)?.keyword ?? null;
}

interface EngineEditorProps {
  /** The engine edited, or `null` for a new one. */
  listed: ListedSearchEngine | null;
  update: UpdateSettings;
  onDone: () => void;
}

/**
 * An engine opened in place, or the new engine's form. A preset edits only its
 * keyword. Rust drops an engine whose address cannot search, and a keyword
 * another engine has, so either is said under the form rather than vanishing;
 * an edit Rust dropped is put back as it was.
 */
function EngineEditor({ listed, update, onDone }: EngineEditorProps) {
  const nameId = useId();
  const urlId = useId();
  const keywordId = useId();
  const messageId = useId();
  const preset = listed?.preset ?? false;
  const [draft, setDraft] = useState<EngineDraft>({
    name: listed?.engine.name ?? "",
    url: listed?.engine.url ?? "",
    keyword: listed?.engine.keyword ?? "",
  });
  const [message, setMessage] = useState<string | null>(null);
  const set = (field: keyof EngineDraft) => (event: React.ChangeEvent<HTMLInputElement>) => {
    setDraft((current) => ({ ...current, [field]: event.target.value }));
    setMessage(null);
  };

  const save = async () => {
    setMessage(null);
    const keyword = draft.keyword.trim() === "" ? null : draft.keyword.trim();
    const engine: SearchEngine = {
      ...(listed?.engine ?? { id: `custom-${crypto.randomUUID()}`, suggestUrl: null }),
      name: preset ? (listed?.engine.name ?? "") : draft.name.trim(),
      url: preset ? (listed?.engine.url ?? "") : draft.url.trim(),
      keyword,
    };
    const result = await tryUpdateSettings((current) =>
      listed === null
        ? { ...current, customSearchEngines: [...current.customSearchEngines, engine] }
        : withEngine(current, listed, engine),
    );
    if (result === null) {
      return;
    }
    if (result.status === "error") {
      setMessage(describeError(result.error));
      return;
    }
    if (!preset && !result.data.customSearchEngines.some((other) => other.id === engine.id)) {
      if (listed !== null) {
        const original = listed.engine;
        await update((current) => ({ ...current, customSearchEngines: [...current.customSearchEngines, original] }));
      }
      setMessage(t("settings.searchEngines.invalidUrl"));
      return;
    }
    if (keyword !== null && keptKeyword(result.data, listed, engine.id, keyword) !== keyword) {
      setMessage(t("settings.searchEngines.keywordTaken").replace("{keyword}", () => keyword));
      return;
    }
    onDone();
  };

  const field = "ipt soft edgeless";
  // Esc in any field closes the form unsaved, as it abandons a draft elsewhere in settings.
  const cancelOnEscape = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onDone();
    }
  };
  return (
    <form
      className="bg-text/5 flex flex-col gap-2 rounded-(--radius-control) p-2"
      aria-label={listed === null ? t("settings.searchEngines.add") : listed.engine.name}
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <div className="flex flex-wrap gap-2">
        {preset ? (
          <span className="flex min-w-36 flex-1 items-center px-1">{listed?.engine.name}</span>
        ) : (
          <>
            <label htmlFor={nameId} className="sr-only">
              {t("settings.searchEngines.name")}
            </label>
            <input
              id={nameId}
              type="text"
              className={`${field} w-36`}
              placeholder={t("settings.searchEngines.name")}
              autoComplete="off"
              onKeyDown={cancelOnEscape}
              // oxlint-disable-next-line jsx-a11y/no-autofocus -- The form opens because the user asked to edit or add an engine.
              autoFocus
              value={draft.name}
              onChange={set("name")}
            />
            <label htmlFor={urlId} className="sr-only">
              {t("settings.searchEngines.url")}
            </label>
            <input
              id={urlId}
              type="text"
              className={`${field} min-w-48 flex-1 font-mono`}
              placeholder="https://example.com/search?q=%s"
              spellCheck={false}
              autoComplete="off"
              onKeyDown={cancelOnEscape}
              aria-invalid={message !== null}
              aria-describedby={message === null ? undefined : messageId}
              value={draft.url}
              onChange={set("url")}
            />
          </>
        )}
        <label htmlFor={keywordId} className="sr-only">
          {t("settings.searchEngines.keywordField")}
        </label>
        <input
          id={keywordId}
          type="text"
          className={`${field} w-24 font-mono`}
          placeholder={t("settings.searchEngines.keywordField")}
          spellCheck={false}
          autoComplete="off"
          onKeyDown={cancelOnEscape}
          // oxlint-disable-next-line jsx-a11y/no-autofocus -- A preset's form opens to edit its keyword, its one field.
          autoFocus={preset}
          value={draft.keyword}
          onChange={set("keyword")}
        />
      </div>
      {message !== null && (
        <p id={messageId} role="alert" className="text-destructive text-sm">
          {message}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <button type="button" className="btn ghost" onClick={onDone}>
          {t("dialog.cancel")}
        </button>
        <button type="submit" className="btn" disabled={!preset && draft.url.trim() === ""}>
          {listed === null ? t("settings.searchEngines.addSubmit") : t("settings.searchEngines.save")}
        </button>
      </div>
    </form>
  );
}
