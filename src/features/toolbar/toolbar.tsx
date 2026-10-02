import { type Tab } from "@bindings";
import { commands } from "@ipc/commands";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useEffect, useRef, useState, type FormEvent } from "react";

import { addressIcon, displayAddress } from "./address";

const SETTINGS_URL = "haku://settings";

interface ToolbarProps {
  tab: Tab | null;
}

export function Toolbar({ tab }: ToolbarProps) {
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const url = tab ? (tab.history.entries[tab.history.index]?.url ?? "") : "";
  const canGoBack = (tab?.history.index ?? 0) > 0;
  const canGoForward = tab ? tab.history.index + 1 < tab.history.entries.length : false;

  // The address follows the page, except while it is being edited: overwriting
  // what someone is typing because a page finished loading is maddening.
  useEffect(() => {
    if (!editing) {
      setDraft(displayAddress(url));
    }
  }, [editing, url]);

  // An edit belongs to the tab it was started in. Switching tabs abandons it,
  // so the field never shows one tab's address while another is active.
  const tabId = tab?.id;
  useEffect(() => {
    inputRef.current?.blur();
    setEditing(false);
  }, [tabId]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!tab) {
      return;
    }
    void commands.navigateTab(tab.id, draft);
    // Leaving the field is what ends the edit, as it does in every browser.
    inputRef.current?.blur();
  };

  return (
    <nav className="haku-bar flex items-center gap-0.75 px-1.5 pb-0.75" aria-label={t("toolbar.navigation")}>
      <ToolbarButton
        icon="ic-arrow-left"
        label={t("toolbar.back")}
        disabled={!canGoBack}
        onClick={() => tab && void commands.goBack(tab.id)}
      />
      <ToolbarButton
        icon="ic-arrow-right"
        label={t("toolbar.forward")}
        disabled={!canGoForward}
        onClick={() => tab && void commands.goForward(tab.id)}
      />
      <ToolbarButton
        icon="ic-rotate-cw"
        label={t("toolbar.reload")}
        disabled={!tab}
        onClick={() => tab && void commands.reloadTab(tab.id)}
      />

      {/* An omnibox rather than a form control from @codenhub/styles: its
          shape and resting state are specific to a browser address bar. */}
      <form className="ml-1.5 min-w-0 flex-1" onSubmit={submit}>
        <label className="bg-chrome-raised text-text-secondary focus-within:outline-text/30 flex h-(--control-height) items-center gap-2 rounded-(--radius-control) px-2.5 focus-within:outline-2">
          <i className={cx(addressIcon(url, editing), "ic-sm shrink-0")} aria-hidden="true" />
          <input
            ref={inputRef}
            className="text-text placeholder:text-text-secondary min-w-0 flex-1 bg-transparent outline-none select-text"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onFocus={(event) => {
              setEditing(true);
              setDraft(url);
              // The full URL is swapped in on focus, so selecting has to wait
              // for React to render it.
              const input = event.target;
              requestAnimationFrame(() => input.select());
            }}
            onBlur={() => setEditing(false)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.currentTarget.blur();
              }
            }}
            spellCheck={false}
            autoComplete="off"
            aria-label={t("toolbar.address")}
            placeholder={t("toolbar.address")}
          />
        </label>
      </form>

      <ToolbarButton
        icon="ic-code"
        label={t("toolbar.devtools")}
        disabled={!tab}
        onClick={() => tab && void commands.openTabDevtools(tab.id)}
      />
      <ToolbarButton
        icon="ic-settings"
        label={t("toolbar.settings")}
        onClick={() => void commands.openTab(SETTINGS_URL, true)}
      />
    </nav>
  );
}

interface ToolbarButtonProps {
  icon: string;
  label: string;
  disabled?: boolean;
  onClick: () => void;
}

function ToolbarButton({ icon, label, disabled, onClick }: ToolbarButtonProps) {
  return (
    <button
      type="button"
      className="btn icon ghost shrink-0"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
    >
      <i className={cx(icon, "ic-sm")} aria-hidden="true" />
    </button>
  );
}
