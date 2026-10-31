import { isNewTab, SETTINGS_URL } from "@app/router";
import { type Tab } from "@bindings";
import { DownloadsButton } from "@features/downloads/downloads-button";
import { ExtensionsBar } from "@features/extensions/extensions-bar";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useId, useRef, useState, type FormEvent, type ReactNode } from "react";

import { displayAddress, editableAddress, isInsecure } from "./address";
import { PrivateBadge } from "./private-badge";
import { SiteMenu } from "./site-menu";
import { useAddressFocus } from "./use-address-focus";

interface ToolbarProps {
  tab: Tab | null;
  /**
   * Drawn at the right end, for the window controls when the tabs are in a
   * column: the toolbar is then the window's top row, and a drag region.
   */
  trailing?: ReactNode;
}

export function Toolbar({ tab, trailing }: ToolbarProps) {
  const { private: isPrivate } = useBrowserState();
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const inputId = useId();

  const url = tab ? (tab.history.entries[tab.history.index]?.url ?? "") : "";
  const canGoBack = (tab?.history.index ?? 0) > 0;
  const canGoForward = tab ? tab.history.index + 1 < tab.history.entries.length : false;

  const tabId = tab?.id;
  useAddressFocus(tabId, inputRef);

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
    <nav
      data-toolbar
      className={cx(
        "haku-bar flex items-center gap-0.75 pl-1.5",
        trailing === undefined ? "pr-1.5 pb-0.75" : "h-8.5 shrink-0",
      )}
      aria-label={t("toolbar.navigation")}
      data-tauri-drag-region={trailing === undefined ? undefined : true}
    >
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
        {/* Pointed at the input explicitly: the site menu's button comes first,
            and a label otherwise labels its first labelable descendant. */}
        <label
          htmlFor={inputId}
          className="bg-chrome-raised text-text-secondary focus-within:outline-text/30 flex h-(--control-height) items-center gap-2 rounded-(--radius-control) px-2.5 focus-within:outline-2"
        >
          {/* The icon says what the page is, and stays put while typing: the
              site menu behind it is as useful mid-edit as at rest. */}
          {tab && url !== "" && tab.presence.status !== "internal" ? (
            <SiteMenu key={tab.id} tab={tab} url={url} warnInsecure={!editing && isInsecure(url)} />
          ) : (
            <i
              className={cx(
                tab?.presence.status === "internal" && !isNewTab(url) ? "ic-haku" : "ic-search",
                "ic-sm shrink-0",
              )}
              aria-hidden="true"
            />
          )}
          <input
            ref={inputRef}
            id={inputId}
            className="text-text placeholder:text-text-secondary min-w-0 flex-1 bg-transparent outline-none select-text"
            // The address follows the page, except while it is being edited:
            // overwriting what someone is typing because a page finished
            // loading is maddening.
            value={editing ? draft : displayAddress(url)}
            onChange={(event) => setDraft(event.target.value)}
            onFocus={(event) => {
              setEditing(true);
              setDraft(editableAddress(url));
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

      {isPrivate && <PrivateBadge />}
      {!isPrivate && <ExtensionsBar tab={tabId} />}
      <DownloadsButton />
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
      {trailing !== undefined && <div className="ml-1.5 flex shrink-0 self-stretch">{trailing}</div>}
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
