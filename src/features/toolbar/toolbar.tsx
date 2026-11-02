import { isNewTab, SETTINGS_URL } from "@app/router";
import { type Tab } from "@bindings";
import { BookmarkStar } from "@features/bookmarks/bookmark-star";
import { BookmarksButton } from "@features/bookmarks/bookmarks-button";
import { isBookmarkable } from "@features/bookmarks/tree";
import { DownloadsButton } from "@features/downloads/downloads-button";
import { ExtensionsBar } from "@features/extensions/extensions-bar";
import { useOverlay } from "@features/overlays/use-overlay";
import { SuggestionList } from "@features/search/suggestion-list";
import { completed, rowText, useSuggestions } from "@features/search/suggestions";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useId, useLayoutEffect, useRef, useState, type FormEvent, type KeyboardEvent, type ReactNode } from "react";

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

/**
 * What the toolbar's right end holds, as a row at the foot of the sidebar,
 * with the **Bookmarks** button first when bookmarks are a button.
 */
export function SidebarActions({ tab, bookmarks = false }: { tab: Tab | null; bookmarks?: boolean }) {
  return (
    // Wrapping, so pinned extensions in a narrow column move to a second line
    // rather than push buttons past its edge.
    <div className="flex shrink-0 flex-wrap items-center justify-end gap-(--haku-gap)">
      {bookmarks && <BookmarksButton tab={tab?.id ?? null} />}
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
        onClick={() => void commands.openTab(SETTINGS_URL, true, "other")}
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
  // The address the edit started from, shown until the user types.
  const [draft, setDraft] = useState("");
  const [typed, setTyped] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [selected, setSelected] = useState(0);
  // Whether the last edit added text: completing after a deletion would put
  // back what was just deleted.
  const [completing, setCompleting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const fieldRef = useRef<HTMLLabelElement>(null);
  // The select-all a focus asks for, waiting on a frame.
  const selectingRef = useRef(0);
  const inputId = useId();
  const listId = useId();
  const isExpanded = expandable && editing;
  // Grown, it reaches over the page, which must not take its clicks.
  useOverlay(fieldRef, isExpanded);

  const url = tab ? (tab.history.entries[tab.history.index]?.url ?? "") : "";
  useAddressFocus(tab?.id, inputRef);

  // The last answer's rows stay up until the next arrives, so the list does
  // not flicker with every keystroke; only an answer to what is typed now
  // completes it.
  const suggestions = useSuggestions(editing ? typed : null);
  const rows = suggestions?.rows ?? [];
  const isCurrent = suggestions !== null && suggestions.input === typed;
  const completion =
    typed !== null && completing && isCurrent && selected === 0 ? completed(typed, suggestions.completion) : null;
  const entered = completion ?? typed ?? draft;
  const highlighted = selected > 0 ? rows[selected] : undefined;
  const value = editing ? (highlighted ? rowText(highlighted, entered) : entered) : displayAddress(url);
  const isListed = editing && typed !== null && rows.length > 0;
  const rowId = (index: number) => `${listId}-${index}`;

  // The completed part is selected, so typing on replaces it.
  useLayoutEffect(() => {
    if (typed !== null && completion !== null) {
      inputRef.current?.setSelectionRange(typed.length, completion.length);
    }
  }, [typed, completion]);

  const retype = (next: string, adds: boolean) => {
    setTyped(next);
    setSelected(0);
    setCompleting(adds);
  };

  const navigate = (input: string) => {
    if (tab) {
      void commands.navigateTab(tab.id, input, "typed");
    }
  };

  /** Does what a row says. The first row, and no row, act on the input. */
  const choose = (index: number) => {
    const row = index > 0 ? rows[index] : undefined;
    switch (row?.kind) {
      case "tab": {
        void commands.switchToTab(row.tab);
        break;
      }
      case "page": {
        // Typed, as typing it out would be. [History § Transitions](../../../docs/specs/history.md#transitions)
        navigate(row.url);
        break;
      }
      case "engine": {
        retype(`${row.keyword} `, false);
        return;
      }
      default: {
        navigate(entered);
      }
    }
    // Leaving the field is what ends the edit, as it does in every browser.
    inputRef.current?.blur();
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    choose(isListed ? selected : 0);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if ((event.key === "ArrowDown" || event.key === "ArrowUp") && isListed) {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setSelected((current) => Math.min(Math.max(current + step, 0), rows.length - 1));
      return;
    }
    if (event.key === "Escape") {
      // First back to what was typed, then out of the field.
      if (selected > 0 || completion !== null) {
        event.preventDefault();
        setSelected(0);
        setCompleting(false);
        return;
      }
      event.currentTarget.blur();
    }
  };

  return (
    // An omnibox rather than a form control from @codenhub/styles: its shape
    // and resting state are specific to a browser address bar.
    <form
      className={cx("relative min-w-0", expandable && "h-(--control-height) shrink-0", className)}
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
          // Keyed apart from the star, a sibling also kept per tab: siblings
          // sharing a key are left behind as the tab changes.
          <SiteMenu key={`site-${tab.id}`} tab={tab} url={url} warnInsecure={!editing && isInsecure(url)} />
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
          value={value}
          onChange={(event) => {
            // Typing before the frame would otherwise be selected whole, and
            // the next keystroke would replace it.
            cancelAnimationFrame(selectingRef.current);
            const { inputType } = event.nativeEvent as InputEvent;
            retype(event.target.value, inputType?.startsWith("insert") ?? false);
          }}
          onFocus={(event) => {
            setEditing(true);
            setDraft(editableAddress(url));
            setTyped(null);
            setSelected(0);
            // The full URL is swapped in on focus, so selecting has to wait
            // for React to render it.
            const input = event.target;
            selectingRef.current = requestAnimationFrame(() => input.select());
          }}
          onBlur={() => setEditing(false)}
          onKeyDown={onKeyDown}
          spellCheck={false}
          autoComplete="off"
          role="combobox"
          aria-autocomplete="both"
          aria-expanded={isListed}
          aria-controls={listId}
          aria-activedescendant={isListed ? rowId(selected) : undefined}
          aria-label={t("toolbar.address")}
          placeholder={t("toolbar.address")}
        />
        {tab && tab.zoom !== 100 && tab.presence.status !== "internal" && <ZoomButton tab={tab} />}
        {/* Internal pages and pages with no address have nothing durable to
            return to. [Bookmarks](../../../docs/specs/bookmarks.md#bookmarking-a-page) */}
        {tab && tab.presence.status !== "internal" && isBookmarkable(url) && (
          <BookmarkStar key={`star-${tab.id}`} tab={tab.id} url={url} />
        )}
      </label>
      <SuggestionList
        id={listId}
        rows={isListed ? rows : []}
        selected={selected}
        rowId={rowId}
        onChoose={choose}
        className="absolute inset-x-0 top-full mt-1"
      />
    </form>
  );
}

/**
 * The page's zoom while it is not 100%, which resets it for the whole site.
 * [Zoom](../../../docs/specs/zoom.md).
 */
function ZoomButton({ tab }: { tab: Tab }) {
  const label = t("toolbar.resetZoom").replace("{percent}", String(tab.zoom));
  return (
    <button
      type="button"
      className="btn ghost shrink-0 px-1.5 text-xs tabular-nums"
      aria-label={label}
      title={label}
      onClick={() => void commands.resetZoom(tab.id)}
    >
      {`${tab.zoom}%`}
    </button>
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
