import { isNewTab, SETTINGS_URL } from "@app/router";
import { type Tab } from "@bindings";
import { BookmarkStar } from "@features/bookmarks/bookmark-star";
import { BookmarksButton } from "@features/bookmarks/bookmarks-button";
import { isBookmarkable } from "@features/bookmarks/tree";
import { DownloadsButton } from "@features/downloads/downloads-button";
import { ExtensionsBar } from "@features/extensions/extensions-bar";
import { useOverlay } from "@features/overlays/use-overlay";
import { SuggestionList } from "@features/search/suggestion-list";
import { completed, rowText, useEngineSuggestions, useSuggestions, withEngineRows } from "@features/search/suggestions";
import { usePageBackdrop } from "@features/viewport/backdrop";
import { commands } from "@ipc/commands";
import { useBrowserState } from "@ipc/hooks";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useId, useLayoutEffect, useRef, useState, type FormEvent, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";

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
  /** Edits the address in a card over the window rather than in the toolbar. */
  floatingAddress?: boolean;
}

/** The toolbar as a row across the top of the window. */
export function Toolbar({ tab, trailing, floatingAddress = false }: ToolbarProps) {
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
      <AddressField tab={tab} className="flex-1" floating={floatingAddress} />
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
 * sidebar's full width. The address is always edited in the floating card, so
 * a long address and its suggestions have room however narrow the sidebar is
 * ([Address bar § Layouts](../../../docs/specs/address-bar.md#layouts)).
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
      <AddressField tab={tab} floating />
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
  /**
   * Edited in a card centred over the window, opened by focusing the field,
   * which keeps showing the address meanwhile.
   * [Address bar § Layouts](../../../docs/specs/address-bar.md#layouts).
   */
  floating?: boolean;
}

function AddressField({ tab, className, floating = false }: AddressFieldProps) {
  // The address the edit started from, shown until the user types.
  const [draft, setDraft] = useState("");
  const [typed, setTyped] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [selected, setSelected] = useState(0);
  // Whether the last edit added text: completing after a deletion would put
  // back what was just deleted.
  const [completing, setCompleting] = useState(false);
  // The input being edited: the card's while it is open, the field's otherwise,
  // so focusing it opens the card and leaving it closes the card.
  const inputRef = useRef<HTMLInputElement>(null);
  const cardRef = useRef<HTMLFormElement>(null);
  const backdropRef = useRef<HTMLDivElement>(null);
  // The select-all a focus asks for, waiting on a frame.
  const selectingRef = useRef(0);
  const inputId = useId();
  const listId = useId();
  const isCardOpen = floating && editing;
  // Over the page, which must not take its clicks. It holds no floating bar:
  // it hangs from none, and the address the bar shows is not what is typed.
  useOverlay(cardRef, isCardOpen);
  // Covers the window, so the page under it is drawn from its capture.
  const isBackdropUp = usePageBackdrop(isCardOpen);
  useOverlay(backdropRef, isBackdropUp);

  const url = tab ? (tab.history.entries[tab.history.index]?.url ?? "") : "";
  useAddressFocus(tab?.id, inputRef);

  // The last answer's rows stay up until the next arrives, so the list does
  // not flicker with every keystroke; only an answer to what is typed now
  // completes it.
  const suggestions = useSuggestions(editing ? typed : null);
  const engineRows = useEngineSuggestions(editing ? typed : null);
  // Completions wait for the local rows, so the first row is always what
  // Enter does, which choosing and the selection count on.
  const rows = suggestions ? withEngineRows(suggestions.rows, engineRows) : [];
  const isCurrent = suggestions !== null && suggestions.input === typed;
  const completion =
    typed !== null && completing && isCurrent && selected === 0 ? completed(typed, suggestions.completion) : null;
  const entered = completion ?? typed ?? draft;
  const highlighted = selected > 0 ? rows[selected] : undefined;
  const value = editing ? (highlighted ? rowText(highlighted, entered) : entered) : displayAddress(url);
  const isListed = editing && typed !== null && rows.length > 0;
  const rowId = (index: number) => `${listId}-${index}`;

  // The card's input takes over from the field's as it opens.
  useLayoutEffect(() => {
    if (isCardOpen) {
      inputRef.current?.focus();
    }
  }, [isCardOpen]);

  // The completed part is selected, so typing on replaces it.
  useLayoutEffect(() => {
    if (typed !== null && completion !== null) {
      inputRef.current?.setSelectionRange(typed.length, completion.length);
    }
  }, [typed, completion]);

  const begin = (input: HTMLInputElement) => {
    cancelAnimationFrame(selectingRef.current);
    setEditing(true);
    setDraft(editableAddress(url));
    setTyped(null);
    setSelected(0);
    // The full URL is swapped in on focus, so selecting has to wait for React
    // to render it.
    selectingRef.current = requestAnimationFrame(() => input.select());
  };

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
      case "suggested": {
        // A search, recorded as one. [History § Transitions](../../../docs/specs/history.md#transitions)
        navigate(row.input);
        break;
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

  const editor = (
    <input
      ref={inputRef}
      id={floating ? undefined : inputId}
      className="text-text placeholder:text-text-secondary min-w-0 flex-1 truncate bg-transparent outline-none select-text"
      // The address follows the page, except while it is being edited:
      // overwriting what someone is typing because a page finished loading is
      // maddening.
      value={value}
      onChange={(event) => {
        // Typing before the frame would otherwise be selected whole, and the
        // next keystroke would replace it.
        cancelAnimationFrame(selectingRef.current);
        const { inputType } = event.nativeEvent as InputEvent;
        retype(event.target.value, inputType?.startsWith("insert") ?? false);
      }}
      onFocus={(event) => begin(event.target)}
      // Leaving the card's input, by a click outside or the window losing
      // focus, closes the card.
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
  );

  return (
    // An omnibox rather than a form control from @codenhub/styles: its shape
    // and resting state are specific to a browser address bar.
    <form
      className={cx("relative min-w-0", className)}
      // Floating, the card submits; the field at rest has nothing to submit.
      onSubmit={floating ? (event) => event.preventDefault() : submit}
    >
      {/* Pointed at the input explicitly: the site menu's button comes first,
          and a label otherwise labels its first labelable descendant. */}
      <label
        htmlFor={inputId}
        className="bg-chrome-raised text-text-secondary focus-within:outline-text/30 flex h-(--control-height) items-center gap-2 rounded-(--radius-control) px-2.5 focus-within:outline-2"
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
        {floating ? (
          // Shows the address and opens the card; the card's input is the
          // one typed into.
          <input
            ref={isCardOpen ? undefined : inputRef}
            id={inputId}
            className="text-text placeholder:text-text-secondary min-w-0 flex-1 truncate bg-transparent outline-none"
            value={displayAddress(url)}
            readOnly
            onFocus={(event) => begin(event.target)}
            aria-label={t("toolbar.address")}
            placeholder={t("toolbar.address")}
          />
        ) : (
          editor
        )}
        {tab && tab.zoom !== 100 && tab.presence.status !== "internal" && <ZoomButton tab={tab} />}
        {/* Internal pages and pages with no address have nothing durable to
            return to. [Bookmarks](../../../docs/specs/bookmarks.md#bookmarking-a-page) */}
        {tab && tab.presence.status !== "internal" && isBookmarkable(url) && (
          <BookmarkStar key={`star-${tab.id}`} tab={tab.id} url={url} />
        )}
      </label>
      {!floating && (
        <SuggestionList
          id={listId}
          rows={isListed ? rows : []}
          selected={selected}
          rowId={rowId}
          onChoose={choose}
          className="border-edge absolute inset-x-0 top-full mt-1 rounded-(--radius-control) border"
        />
      )}
      {isCardOpen &&
        createPortal(
          // One portal, so the backdrop, arriving after the card, still goes
          // in under it rather than on top.
          <>
            {isBackdropUp && (
              // Pressing it leaves the card's input, which closes the card.
              <div ref={backdropRef} data-address-backdrop className="haku-address-backdrop" aria-hidden="true" />
            )}
            {/* Centred on the window rather than the page, which a docked
                column would put off-centre. Opaque, edged and without a
                shadow, as a floating bar is, since the page cannot show
                through or around it. */}
            {/* oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- The press is not handled, only kept from taking focus from the input. */}
            <form
              ref={cardRef}
              data-address-card
              className="bg-chrome-raised border-text/15 text-text fixed top-[20vh] left-1/2 z-30 flex w-[min(48rem,calc(100vw-6rem))] -translate-x-1/2 flex-col overflow-hidden rounded-(--haku-radius) border"
              onSubmit={submit}
              // Pressing anywhere in the card but the input would blur it, which
              // closes the card.
              onMouseDown={(event) => {
                if (event.target !== inputRef.current) {
                  event.preventDefault();
                }
              }}
            >
              <div className="flex h-[calc(var(--control-height)+0.5rem)] items-center gap-2 px-3">
                <i className="ic-search ic-sm text-text-secondary shrink-0" aria-hidden="true" />
                {editor}
              </div>
              <SuggestionList
                id={listId}
                rows={isListed ? rows : []}
                selected={selected}
                rowId={rowId}
                onChoose={choose}
                className="border-edge border-t"
              />
            </form>
          </>,
          document.body,
        )}
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
