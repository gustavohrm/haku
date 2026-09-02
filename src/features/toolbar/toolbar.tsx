import { commands, type Tab } from "@bindings";
import { t } from "@shared/i18n";
import { useEffect, useState, type FormEvent } from "react";

const SETTINGS_URL = "haku:settings";

interface ToolbarProps {
  tab: Tab | null;
}

export function Toolbar({ tab }: ToolbarProps) {
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState(false);

  const url = tab ? (tab.history.entries[tab.history.index]?.url ?? "") : "";
  const canGoBack = (tab?.history.index ?? 0) > 0;
  const canGoForward = tab ? tab.history.index + 1 < tab.history.entries.length : false;

  // The address follows the page, except while it is being edited: overwriting
  // what someone is typing because a page finished loading is maddening.
  useEffect(() => {
    if (!editing) {
      setDraft(url);
    }
  }, [editing, url]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!tab) {
      return;
    }
    void commands.navigateTab(tab.id, draft);
    setEditing(false);
  };

  return (
    <nav className="haku-toolbar" aria-label={t("toolbar.address")}>
      <button
        type="button"
        className="haku-icon-button"
        aria-label={t("toolbar.back")}
        title={t("toolbar.back")}
        disabled={!canGoBack}
        onClick={() => tab && void commands.goBack(tab.id)}
      >
        <i className="ic-arrow-left" aria-hidden="true" />
      </button>
      <button
        type="button"
        className="haku-icon-button"
        aria-label={t("toolbar.forward")}
        title={t("toolbar.forward")}
        disabled={!canGoForward}
        onClick={() => tab && void commands.goForward(tab.id)}
      >
        <i className="ic-arrow-right" aria-hidden="true" />
      </button>
      <button
        type="button"
        className="haku-icon-button"
        aria-label={t("toolbar.reload")}
        title={t("toolbar.reload")}
        disabled={!tab}
        onClick={() => tab && void commands.reloadTab(tab.id)}
      >
        <i className="ic-rotate-cw" aria-hidden="true" />
      </button>

      <form className="min-w-0 flex-1" onSubmit={submit}>
        <input
          className="haku-address"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onFocus={(event) => {
            setEditing(true);
            event.target.select();
          }}
          onBlur={() => setEditing(false)}
          spellCheck={false}
          autoComplete="off"
          aria-label={t("toolbar.address")}
          placeholder={t("toolbar.address")}
        />
      </form>

      <button
        type="button"
        className="haku-icon-button"
        aria-label={t("toolbar.devtools")}
        title={t("toolbar.devtools")}
        disabled={!tab}
        onClick={() => tab && void commands.openTabDevtools(tab.id)}
      >
        <i className="ic-code" aria-hidden="true" />
      </button>
      <button
        type="button"
        className="haku-icon-button"
        aria-label={t("toolbar.settings")}
        title={t("toolbar.settings")}
        onClick={() => void commands.openTab(SETTINGS_URL, true)}
      >
        <i className="ic-settings" aria-hidden="true" />
      </button>
    </nav>
  );
}
