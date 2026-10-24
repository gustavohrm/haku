import { crashStyle } from "@app/crash-style";
import { InternalPage } from "@app/internal-page";
import { type InternalPageProps } from "@app/router";
import { reportUnexpected } from "@features/feedback/feedback";
import { commands } from "@ipc/commands";
import { useSettings } from "@ipc/hooks";
import { reloadSettings, updateSettings } from "@ipc/store";
import { cx } from "@shared/class-names";
import { t } from "@shared/i18n";
import { useState } from "react";

import logoUrl from "../../../assets/logo/logo-light.svg?url";
import { GroupPlate, SettingRow, SettingsGroup } from "./rows";
import { searchSettings, SECTIONS, sectionOf, sectionUrl } from "./sections";
import { type SettingContext, type UpdateSettings } from "./types";

/**
 * Settings, rendered inside the chrome rather than in a webview.
 *
 * A sidebar of sections, each at its own address so the address field shows
 * where you are, Back returns to the previous section, and anything can link
 * to one. A search box filters rows across every section; it is not part of the
 * address, because a query is a moment, not a place.
 *
 * Changes are written through to Rust, which persists them and applies
 * anything with an immediate effect, such as destroying webviews when the pool
 * shrinks. Typed values are saved when the field is left or Enter is pressed,
 * not per keystroke: Rust sanitises what it receives, and a half-typed value
 * would be corrected out from under the person typing it.
 */
export function SettingsPage({ tabId, url }: InternalPageProps) {
  const settings = useSettings();
  const [query, setQuery] = useState("");
  if (!settings) {
    return <SettingsUnavailable />;
  }

  const update: UpdateSettings = (change) =>
    updateSettings((current) => (typeof change === "function" ? change(current) : { ...current, ...change }));
  const context: SettingContext = { settings, update };
  const current = sectionOf(url);
  const searching = query.trim() !== "";
  const results = searching ? searchSettings(query) : [];

  const sidebar = (
    <>
      <span className="text-title-sm px-2">{t("settings.title")}</span>
      {/* Sunk into the sidebar rather than outlined: it is the only field there. */}
      <label className="input-group soft edgeless unadorned">
        <i className="ic-search ic-sm text-text-secondary" aria-hidden="true" />
        <input
          type="search"
          className="ipt"
          placeholder={t("settings.search.placeholder")}
          aria-label={t("settings.search.placeholder")}
          value={query}
          spellCheck={false}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              setQuery("");
            }
          }}
        />
      </label>
      <nav aria-label={t("settings.sections")}>
        <ul className="flex flex-wrap gap-1 @3xl:flex-col">
          {SECTIONS.map((section) => {
            const active = !searching && section === current;
            return (
              <li key={section.id}>
                <button
                  type="button"
                  className={cx(
                    "flex h-9 w-full items-center gap-2.5 rounded-(--radius-control) px-2.5 text-left transition-colors",
                    active ? "bg-surface text-text" : "text-text-secondary hover:bg-text/6 hover:text-text",
                  )}
                  aria-current={active ? "page" : undefined}
                  onClick={() => {
                    setQuery("");
                    if (section !== current) {
                      void commands.navigateTab(tabId, sectionUrl(section));
                    }
                  }}
                >
                  <i className={cx(section.icon, "ic-md shrink-0")} aria-hidden="true" />
                  {t(section.title)}
                </button>
              </li>
            );
          })}
        </ul>
      </nav>
    </>
  );

  if (searching) {
    return (
      <InternalPage title={t("settings.search.results")} sidebar={sidebar}>
        {results.length === 0 && <p className="text-text-secondary">{t("settings.search.empty")}</p>}
        {results.map(({ section, rows }) => (
          <GroupPlate key={section.id} title={t(section.title)}>
            {rows.map((row) => (
              <SettingRow key={row.id} row={row} context={context} />
            ))}
          </GroupPlate>
        ))}
      </InternalPage>
    );
  }

  return (
    <InternalPage title={t(current.title)} sidebar={sidebar}>
      {current.id === "about" && <AboutHeader />}
      {current.groups.map((group, index) => (
        // Groups have no identity beyond their place in a fixed list.
        // oxlint-disable-next-line react/no-array-index-key
        <SettingsGroup key={index} group={group} context={context} />
      ))}
    </InternalPage>
  );
}

/**
 * The full logo, painted in the text colour through a mask so it suits either
 * theme. Decorative: the page's heading already names it.
 */
function AboutHeader() {
  return (
    <div
      aria-hidden="true"
      className="bg-text my-2 aspect-963/256 h-12 self-start"
      // Prefixed too: the bundle targets Chromium 110, which masks only with it.
      style={{
        WebkitMask: `url("${logoUrl}") center / contain no-repeat`,
        mask: `url("${logoUrl}") center / contain no-repeat`,
      }}
    />
  );
}

/**
 * Shown when the settings could not be read, which leaves nothing to draw the
 * rows from. Why was already reported when loading failed.
 */
function SettingsUnavailable() {
  return (
    <div className={crashStyle.page}>
      <div className={crashStyle.notice}>
        <i className={crashStyle.mark} aria-hidden="true" />
        <h1 className={crashStyle.title}>{t("settings.unavailable")}</h1>
        <p className={crashStyle.description}>{t("settings.unavailable.help")}</p>
        <button
          type="button"
          className={crashStyle.action}
          onClick={async () => {
            try {
              await reloadSettings();
            } catch (error) {
              reportUnexpected(error);
            }
          }}
        >
          {t("error.retry")}
        </button>
      </div>
    </div>
  );
}
