import { type HistoryRule, type Tab } from "@bindings";
import { useBarHold } from "@features/bars/holds";
import {
  countVisits,
  HISTORY_RULES_URL,
  neverRecordOffer,
  neverRecordSite,
  recordSiteAgain,
  ruleNote,
  siteFilter,
  siteRuleFor,
  type SiteRule,
} from "@features/history/history-actions";
import { MENU_GAP, useMenuDismiss } from "@features/overlays/use-menu-dismiss";
import { useOverlay } from "@features/overlays/use-overlay";
import { commands } from "@ipc/commands";
import { useBrowserState, useSettings } from "@ipc/hooks";
import { updateSettings } from "@ipc/store";
import { t } from "@shared/i18n";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

interface SiteMenuProps {
  tab: Tab;
  url: string;
  /** Say on the button itself that the page is not secure. */
  warnInsecure: boolean;
}

/**
 * The address bar's leading icon, and the menu it opens about the site.
 *
 * The icon is the same on every site: it opens the site's settings, and a lock
 * beside a URL reads as a verdict on the site that it never was. What a lock
 * used to say is in the menu, and a page on plain HTTP also says it beside the
 * button, as other browsers do.
 *
 * Keeping a tab or its site loaded overrides the optimization settings, so
 * turning either on says what it costs, in place, and asks before it takes
 * effect; turning it off asks nothing. Keeping the site applies to every tab
 * on its host, so it is stored in the settings rather than on the tab.
 * [Webview pool § Fixed tabs](../../../docs/specs/webview-pool.md#fixed-tabs).
 */
export function SiteMenu({ tab, url, warnInsecure }: SiteMenuProps) {
  // Where the menu opens, or null while closed.
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  const open = anchor !== null;
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLElement>(null);
  const menuId = useId();
  const { private: isPrivate, historyPaused } = useBrowserState();
  const settings = useSettings();
  const host = hostnameOf(url);
  const siteKept = host !== null && (settings?.keptSites.includes(host) ?? false);
  const siteRule = useSiteRule(open && !isPrivate ? url : null, host, settings?.historyRules);

  const setSiteKept = (kept: boolean) => {
    if (isPrivate || host === null) {
      return;
    }
    void updateSettings((current) => {
      const others = current.keptSites.filter((site) => site !== host);
      return { ...current, keptSites: kept ? [...others, host] : others };
    });
  };

  // The menu hangs over the page, so it must be registered to be clickable.
  useOverlay(menuRef, open);
  useBarHold("toolbar", open);

  const close = useCallback(() => setAnchor(null), []);
  useMenuDismiss(open, close, menuRef, buttonRef);

  // The menu is anchored at the glyph rather than its button, whose hover box
  // is wider than the glyph, and pulls itself back by its own inset, border,
  // padding and a row's padding, so its text starts directly under the glyph.
  const toggle = () => {
    const button = buttonRef.current?.getBoundingClientRect();
    const glyph = buttonRef.current?.querySelector("i")?.getBoundingClientRect();
    setAnchor(open || !button || !glyph ? null : { x: glyph.left, y: button.bottom + MENU_GAP });
  };

  const secure = url.startsWith("https://");

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        className="btn icon ghost p-xs -mx-1 shrink-0 [--ui-radius:var(--radius-small)]"
        aria-label={t("site.menu")}
        title={t("site.menu")}
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={toggle}
      >
        <i className="ic-sliders-horizontal ic-sm" aria-hidden="true" />
      </button>
      {/* Beside the button rather than in it, so the button keeps the same
          square shape as every other icon button. */}
      {warnInsecure && <span className="text-text-secondary shrink-0">{t("site.notSecure")}</span>}

      {/*
        Portalled out of the address bar: inside its <label>, any click in the
        menu would focus the field. On the overlay surface, without a shadow,
        which the input mask would cut off at its edge. Its rounded corners are
        cut into the mask from its own style.
      */}
      {anchor &&
        createPortal(
          <section
            ref={menuRef}
            id={menuId}
            aria-label={t("site.menu")}
            tabIndex={-1}
            className="haku-overlay fixed z-10 -ml-[calc(--spacing(3)+1px)] flex w-80 flex-col gap-2 p-1 outline-none"
            style={{ left: anchor.x, top: anchor.y }}
          >
            <div className="flex min-w-0 flex-col px-2 pt-1.5">
              <span className="truncate font-medium">{hostOf(url)}</span>
              <span className="text-text-secondary text-sm">{secure ? t("site.secure") : t("site.insecure")}</span>
            </div>

            <div className="flex flex-col">
              <KeepSwitch
                label={t("site.keepLoaded")}
                help={t("site.keepLoaded.help")}
                warning={t("site.keepLoaded.warning")}
                checked={tab.fixed}
                onChange={(fixed) => void commands.setTabFixed(tab.id, fixed)}
              />
              {!isPrivate && host !== null && (
                <KeepSwitch
                  label={t("site.keepSite")}
                  help={t("site.keepSite.help")}
                  warning={t("site.keepSite.warning")}
                  checked={siteKept}
                  disabled={!settings}
                  onChange={setSiteKept}
                />
              )}
            </div>

            {/* Hidden in a private window, which records nothing, and where a
                rule naming the host would write the private visit to disk. */}
            {!isPrivate && host !== null && (
              <div className="flex flex-col">
                {settings?.historyEnabled === false ? (
                  <span className="text-text-secondary px-2 py-1 text-sm">{t("site.history.off")}</span>
                ) : historyPaused ? (
                  <span className="text-text-secondary px-2 py-1 text-sm">{t("site.history.paused")}</span>
                ) : null}
                <button
                  type="button"
                  className="haku-row min-h-8"
                  onClick={() => {
                    close();
                    void commands.openTab(`haku://history?site=${encodeURIComponent(host)}`, true, "other");
                  }}
                >
                  <i className="ic-history ic-sm" aria-hidden="true" />
                  {t("site.history.show")}
                </button>
                {siteRule !== undefined && (
                  <RuleActions
                    host={host}
                    url={url}
                    siteRule={siteRule}
                    run={(action) => {
                      close();
                      action();
                    }}
                  />
                )}
              </div>
            )}
          </section>,
          document.body,
        )}
    </>
  );
}

interface KeepSwitchProps {
  label: string;
  /** Says what the switch reaches, always shown under it. */
  help: string;
  /** What keeping costs, said in place before it is turned on. */
  warning: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}

/**
 * A Keep loaded switch, with what it reaches muted under it. Turning it on
 * first shows what it costs, with **Keep loaded** and **Cancel**, and takes
 * effect only on **Keep loaded**; turning it off goes at once.
 * [Interface guidelines § Forms](../../../docs/guidelines/interface.md#forms).
 */
function KeepSwitch({ label, help, warning, checked, disabled = false, onChange }: KeepSwitchProps) {
  const switchId = useId();
  const labelId = useId();
  const helpId = useId();
  const warningId = useId();
  const [asking, setAsking] = useState(false);

  return (
    <div className="flex flex-col">
      <label htmlFor={switchId} className="haku-row min-h-8 py-1">
        <span className="flex min-w-0 flex-1 flex-col">
          <span id={labelId}>{label}</span>
          <span id={helpId} className="text-text-secondary text-sm">
            {help}
          </span>
        </span>
        <input
          id={switchId}
          type="checkbox"
          className="switch shrink-0"
          aria-labelledby={labelId}
          aria-describedby={asking ? `${helpId} ${warningId}` : helpId}
          checked={checked || asking}
          disabled={disabled}
          onChange={(event) => {
            if (!event.target.checked) {
              setAsking(false);
              if (checked) {
                onChange(false);
              }
            } else if (!checked) {
              setAsking(true);
            }
          }}
        />
      </label>
      {asking && (
        <div role="alert" className="flex flex-col gap-2 px-2 pt-1 pb-2 text-sm">
          <p id={warningId}>{warning}</p>
          <div className="flex justify-end gap-2">
            <button type="button" className="btn ghost sm" onClick={() => setAsking(false)}>
              {t("dialog.cancel")}
            </button>
            <button
              type="button"
              className="btn sm"
              onClick={() => {
                setAsking(false);
                onChange(true);
              }}
            >
              {t("site.keepLoaded.confirm")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Which rule decides `url`, asked while the menu is open and again when the
 * rules change, so the menu names the rule as it is now. `undefined` until
 * the answer for this page and these rules arrives; `null` when it could not
 * be read.
 *
 * @param url - The page, or `null` while nothing should be asked.
 */
function useSiteRule(
  url: string | null,
  host: string | null,
  rules: readonly HistoryRule[] | undefined,
): SiteRule | null | undefined {
  const key = url === null || host === null ? null : `${url} ${JSON.stringify(rules ?? [])}`;
  const [answer, setAnswer] = useState<{ key: string; rule: SiteRule | null } | null>(null);
  useEffect(() => {
    if (key === null || url === null || host === null) {
      return;
    }
    let current = true;
    void (async () => {
      const rule = await siteRuleFor(url, host);
      if (current) {
        setAnswer({ key, rule });
      }
    })();
    return () => {
      current = false;
    };
  }, [key, url, host]);
  return answer !== null && answer.key === key ? answer.rule : undefined;
}

interface RuleActionsProps {
  host: string;
  url: string;
  siteRule: SiteRule | null;
  /** Closes the menu, then does the action. */
  run: (action: () => void) => void;
}

/**
 * Never record this site, or its undo while the site's own rule decides it.
 * Another rule is named, and when it already stops the site, the way to it in
 * settings takes the place of adding a rule that would change nothing.
 *
 * Never record counts what is already recorded and, when there is some, asks
 * in place under the item, in the settings form's words, whether to delete it
 * too; the menu stays open, and then offers the undo.
 * [History § Elsewhere](../../../docs/specs/history.md#elsewhere).
 */
function RuleActions({ host, url, siteRule, run }: RuleActionsProps) {
  // The visits already recorded, while the offer to delete them shows.
  const [offer, setOffer] = useState<number | null>(null);
  const offerId = useId();

  if (siteRule?.kind === "own") {
    return (
      <button type="button" className="haku-row min-h-8" onClick={() => run(() => void recordSiteAgain(host, url))}>
        <i className="ic-history ic-sm" aria-hidden="true" />
        {t("site.history.recordAgain")}
      </button>
    );
  }
  const other = siteRule?.kind === "other" ? siteRule.rule : null;
  if (other?.action === "never") {
    return (
      <>
        <span className="text-text-secondary px-2 py-1 text-sm">{ruleNote(other)}</span>
        <button
          type="button"
          className="haku-row min-h-8"
          onClick={() => run(() => void commands.openTab(HISTORY_RULES_URL, true, "other"))}
        >
          <i className="ic-settings ic-sm" aria-hidden="true" />
          {t("site.history.editRules")}
        </button>
      </>
    );
  }

  const neverRecord = async () => {
    if (offer !== null) {
      setOffer(null);
      return;
    }
    const count = await countVisits(siteFilter(host));
    if (count === 0) {
      await neverRecordSite(host, false);
    } else if (count !== null) {
      setOffer(count);
    }
  };
  const choose = (deleteRecorded: boolean) => {
    setOffer(null);
    void neverRecordSite(host, deleteRecorded);
  };
  const words = offer === null ? null : neverRecordOffer(offer);

  return (
    <>
      {other && <span className="text-text-secondary px-2 py-1 text-sm">{ruleNote(other)}</span>}
      <button
        type="button"
        className="haku-row min-h-8"
        aria-expanded={offer !== null}
        aria-controls={offer === null ? undefined : offerId}
        onClick={() => void neverRecord()}
      >
        <i className="ic-circle-off ic-sm" aria-hidden="true" />
        {t("site.history.neverRecord")}
      </button>
      {words && (
        <div id={offerId} role="alert" className="flex flex-col gap-2 px-2 pt-1 pb-2 text-sm">
          <p>{words.message}</p>
          <div className="flex flex-wrap justify-end gap-2">
            <button type="button" className="btn soft edgeless sm" onClick={() => choose(false)}>
              {words.keep}
            </button>
            <button type="button" className="btn destructive sm" onClick={() => choose(true)}>
              {words.delete}
            </button>
          </div>
        </div>
      )}
    </>
  );
}

/**
 * A web page's host name as kept sites list it: lowercase, without a port.
 * Nothing for a page that is not on the web.
 */
function hostnameOf(url: string): string | null {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:" ? parsed.hostname || null : null;
  } catch {
    return null;
  }
}

function hostOf(url: string): string {
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
}
