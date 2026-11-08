import { type HistoryRule, type Tab } from "@bindings";
import { useBarHold } from "@features/bars/holds";
import {
  HISTORY_SETTINGS_URL,
  neverRecordSite,
  recordSiteAgain,
  ruleNote,
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
 * The address field's leading icon, and the menu it opens about the site.
 *
 * The icon is the same on every site: it opens the site's settings, and a lock
 * beside a URL reads as a verdict on the site that it never was. What a lock
 * used to say is in the menu, and a page on plain HTTP also says it beside the
 * button, as other browsers do.
 *
 * Keeping a tab loaded lives here rather than on the tab itself: it overrides
 * the optimization settings, so it should take a deliberate step, with the
 * warning beside it. Keeping the site applies to every tab on its host, so it
 * is stored in the settings rather than on the tab.
 */
export function SiteMenu({ tab, url, warnInsecure }: SiteMenuProps) {
  // Where the menu opens, or null while closed.
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  const open = anchor !== null;
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLElement>(null);
  const menuId = useId();
  const switchId = useId();
  const siteSwitchId = useId();
  const siteHelpId = useId();
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
  // is wider than the glyph, and pulls itself back by its own inset so its
  // text starts directly under the glyph.
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
        Portalled out of the address field: inside its <label>, any click in
        the menu would focus the field. On the overlay surface, which lets a
        little of the page through, and without a shadow, which the input mask
        would cut off at its edge. Its rounded corners are cut into the mask
        from its own style.
      */}
      {anchor &&
        createPortal(
          <section
            ref={menuRef}
            id={menuId}
            aria-label={t("site.menu")}
            tabIndex={-1}
            className="bg-chrome-overlay border-text/15 text-text fixed z-10 -ml-[calc(--spacing(3)+1px)] flex w-80 flex-col gap-3 rounded-(--radius-surface) border p-3 outline-none"
            style={{ left: anchor.x, top: anchor.y }}
          >
            <div className="flex min-w-0 flex-col">
              <span className="truncate font-medium">{hostOf(url)}</span>
              <span className="text-text-secondary text-sm">{secure ? t("site.secure") : t("site.insecure")}</span>
            </div>

            {/* The warning sits outside the label: a label takes its first
                labelable descendant as its control, which would be the
                warning's button rather than the switch. */}
            <div className="flex items-center gap-1.5">
              <label htmlFor={switchId}>{t("site.keepLoaded")}</label>
              <Warning message={t("site.keepLoaded.warning")} />
              <input
                id={switchId}
                type="checkbox"
                className="switch ml-auto"
                checked={tab.fixed}
                onChange={(event) => void commands.setTabFixed(tab.id, event.target.checked)}
              />
            </div>

            {!isPrivate && host !== null && (
              <div className="flex flex-col gap-1">
                <div className="flex items-center gap-1.5">
                  <label htmlFor={siteSwitchId}>{t("site.keepSite")}</label>
                  <input
                    id={siteSwitchId}
                    type="checkbox"
                    className="switch ml-auto"
                    aria-describedby={siteHelpId}
                    checked={siteKept}
                    disabled={!settings}
                    onChange={(event) => setSiteKept(event.target.checked)}
                  />
                </div>
                <span id={siteHelpId} className="text-text-secondary text-sm">
                  {t("site.keepSite.help")}
                </span>
              </div>
            )}

            {/* Hidden in a private window, which records nothing, and where a
                rule naming the host would write the private visit to disk. */}
            {!isPrivate && host !== null && (
              <div className="border-text/10 flex flex-col gap-1 border-t pt-3">
                {settings?.historyEnabled === false ? (
                  <span className="text-text-secondary text-sm">{t("site.history.off")}</span>
                ) : historyPaused ? (
                  <span className="text-text-secondary text-sm">{t("site.history.paused")}</span>
                ) : null}
                <button
                  type="button"
                  className="btn ghost justify-start"
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
 * [History § Elsewhere](../../../docs/specs/history.md#elsewhere).
 */
function RuleActions({ host, url, siteRule, run }: RuleActionsProps) {
  if (siteRule?.kind === "own") {
    return (
      <button
        type="button"
        className="btn ghost justify-start"
        onClick={() => run(() => void recordSiteAgain(host, url))}
      >
        <i className="ic-history ic-sm" aria-hidden="true" />
        {t("site.history.recordAgain")}
      </button>
    );
  }
  const other = siteRule?.kind === "other" ? siteRule.rule : null;
  return (
    <>
      {other && <span className="text-text-secondary px-1 text-sm">{ruleNote(other)}</span>}
      {other?.action === "never" ? (
        <button
          type="button"
          className="btn ghost justify-start"
          onClick={() => run(() => void commands.openTab(HISTORY_SETTINGS_URL, true, "other"))}
        >
          <i className="ic-settings ic-sm" aria-hidden="true" />
          {t("site.history.editRules")}
        </button>
      ) : (
        <button type="button" className="btn ghost justify-start" onClick={() => run(() => void neverRecordSite(host))}>
          <i className="ic-circle-off ic-sm" aria-hidden="true" />
          {t("site.history.neverRecord")}
        </button>
      )}
    </>
  );
}

/**
 * A warning icon that explains itself on hover or focus.
 *
 * The bubble can reach past the menu, and anything outside a registered
 * overlay is drawn under the page's hole in the input mask, so it would be
 * invisible there. Whether it is open is therefore tracked here rather than
 * left to CSS, and the bubble is registered while it shows; `useOverlay`
 * measures it again once it has finished scaling up.
 */
function Warning({ message }: { message: string }) {
  const [open, setOpen] = useState(false);
  const bubbleRef = useRef<HTMLSpanElement>(null);
  const bubbleId = useId();

  useOverlay(bubbleRef, open);

  return (
    <span
      className="tooltip flex"
      data-state={open ? "open" : "closed"}
      onPointerEnter={() => setOpen(true)}
      onPointerLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
    >
      <button
        type="button"
        className="text-warning flex outline-offset-2"
        aria-label={t("site.warning")}
        aria-describedby={bubbleId}
      >
        <i className="ic-triangle-alert ic-xs" aria-hidden="true" />
      </button>
      <span ref={bubbleRef} id={bubbleId} role="tooltip" className="tooltip-bubble warning soft edged">
        {message}
      </span>
    </span>
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
