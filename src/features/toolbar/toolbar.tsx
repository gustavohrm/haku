import { isNewTab, SETTINGS_URL } from "@app/router";
import { type Tab } from "@bindings";
import { DownloadsButton } from "@features/downloads/downloads-button";
import { ExtensionsBar } from "@features/extensions/extensions-bar";
import { useOverlay } from "@features/overlays/use-overlay";
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

/** The toolbar as a row across the top of the window. */
export function Toolbar({ tab, trailing }: ToolbarProps) {
  return (
    <nav
      data-toolbar
      className={cx(
        "haku-bar flex items-center gap-(--haku-gap) pl-(--haku-gap)",
        trailing === undefined
          ? "py-(--haku-gap) pr-(--haku-gap)"
          : "h-[calc(var(--control-height)+2*var(--haku-gap))] shrink-0",
      )}
      aria-label={t("toolbar.navigation")}
      data-tauri-drag-region={trailing === undefined ? undefined : true}
    >
      <div className="flex shrink-0 items-center gap-(--haku-gap)">
        <NavigationButtons tab={tab} />
      </div>
      <AddressField tab={tab} className="flex-1" />
      <div className="flex shrink-0 items-center gap-(--haku-gap)">
        <ToolbarActions tab={tab} />
      </div>
      {trailing !== undefined && <div className="flex shrink-0 self-stretch">{trailing}</div>}
    </nav>
  );
}

/**
 * The toolbar at the head of the sidebar: navigation on the first row, whose
 * free space is the window's drag region, and the address under it at the
 * sidebar's full width. The address grows over the page while it is edited,
 * so a long address has room however narrow the sidebar is.
 *
 * It draws no window controls: whoever picks this layout wants the space,
 * and the window is still maximized by double-clicking the sidebar's free
 * space, minimized with Win+Down and closed with Alt+F4 or from the taskbar.
 * [Appearance § Sidebar](../../../docs/specs/appearance.md#sidebar).
 */
export function SidebarToolbar({ tab }: { tab: Tab | null }) {
  return (
    <nav data-toolbar className="flex shrink-0 flex-col gap-(--haku-gap)" aria-label={t("toolbar.navigation")}>
      <div className="flex items-center gap-(--haku-gap)" data-tauri-drag-region>
        <NavigationButtons tab={tab} />
      </div>
      <AddressField tab={tab} expandable />
    </nav>
  );
}

/** What the toolbar's right end holds, as a row at the foot of the sidebar. */
export function SidebarActions({ tab }: { tab: Tab | null }) {
  return (
    <div className="flex shrink-0 items-center justify-end gap-(--haku-gap)">
      <ToolbarActions tab={tab} />
    </div>
  );
}

function NavigationButtons({ tab }: { tab: Tab | null }) {
  const canGoBack = (tab?.history.index ?? 0) > 0;
  const canGoForward = tab ? tab.history.index + 1 < tab.history.entries.length : false;
  return (
    <>
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
    </>
  );
}

function ToolbarActions({ tab }: { tab: Tab | null }) {
  const { private: isPrivate } = useBrowserState();
  return (
    <>
      {isPrivate && <PrivateBadge />}
      {!isPrivate && <ExtensionsBar tab={tab?.id} />}
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
    </>
  );
}

interface AddressFieldProps {
  tab: Tab | null;
  className?: string;
  /** Grows over the page while edited, for an address field in a narrow sidebar. */
  expandable?: boolean;
}

function AddressField({ tab, className, expandable = false }: AddressFieldProps) {
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const fieldRef = useRef<HTMLLabelElement>(null);
  const inputId = useId();
  const isExpanded = expandable && editing;
  // Grown, it reaches over the page, which must not take its clicks.
  useOverlay(fieldRef, isExpanded);

  const url = tab ? (tab.history.entries[tab.history.index]?.url ?? "") : "";
  useAddressFocus(tab?.id, inputRef);

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
    // An omnibox rather than a form control from @codenhub/styles: its shape
    // and resting state are specific to a browser address bar.
    <form
      className={cx("min-w-0", expandable && "relative h-(--control-height) shrink-0", className)}
      onSubmit={submit}
    >
      {/* Pointed at the input explicitly: the site menu's button comes first,
          and a label otherwise labels its first labelable descendant. */}
      <label
        ref={fieldRef}
        htmlFor={inputId}
        data-expanded={isExpanded || undefined}
        className={cx(
          "bg-chrome-raised text-text-secondary focus-within:outline-text/30 flex h-(--control-height) items-center gap-2 rounded-(--radius-control) px-2.5 focus-within:outline-2",
          expandable && "haku-address-expandable",
        )}
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
          className="text-text placeholder:text-text-secondary min-w-0 flex-1 truncate bg-transparent outline-none select-text"
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
