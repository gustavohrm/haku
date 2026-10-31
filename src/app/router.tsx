import { type TabId } from "@bindings";
import { DownloadsPage } from "@features/downloads/downloads-page";
import { HistoryPage } from "@features/history/history-page";
import { MemoryPage } from "@features/memory/memory-page";
import { SettingsPage } from "@features/settings/settings-page";
import { NewTabPage } from "@features/viewport/new-tab-page";
import { t, type TranslationKey } from "@shared/i18n";
import { createElement, type ComponentType, type ReactElement } from "react";

/**
 * Pages Haku renders itself.
 *
 * These live in the chrome rather than in a content webview, which means they
 * cost no webview slot, share the interface's theme and translations, and can
 * talk to Rust directly. The `haku://` scheme is what marks a tab as one of them.
 */
const INTERNAL_PREFIX = "haku://";

/** Where the settings page lives, for anything that opens it. */
export const SETTINGS_URL = "haku://settings";

/** Where every download is listed. */
export const DOWNLOADS_URL = "haku://downloads";

/** What every internal page is rendered with: which tab it is in, and its full URL. */
export interface InternalPageProps {
  tabId: TabId;
  url: string;
}

// Components, not the result of calling them. Invoking a component as a plain
// function runs its hooks inside the caller's hook sequence, so opening or
// leaving an internal page would change that sequence and React would tear the
// whole tree down.
const routes: Record<string, { page: ComponentType<InternalPageProps>; title: TranslationKey }> = {
  "new-tab": { page: NewTabPage, title: "newTab.title" },
  settings: { page: SettingsPage, title: "settings.title" },
  history: { page: HistoryPage, title: "history.title" },
  memory: { page: MemoryPage, title: "memory.title" },
  downloads: { page: DownloadsPage, title: "downloads.title" },
};

export function isInternalUrl(url: string): boolean {
  return url.startsWith(INTERNAL_PREFIX);
}

/**
 * Whether a URL is the new-tab page, which the address field treats as no
 * address at all: there is nothing to show until something is typed.
 */
export function isNewTab(url: string): boolean {
  return routeOf(url) === routes["new-tab"];
}

/**
 * Resolves an internal URL to its page.
 *
 * The page is told its URL and tab rather than reading the active tab, since a
 * window will show more than one page once split view exists.
 *
 * @param url - A `haku://` URL.
 * @param tabId - The tab showing it.
 * @returns The page element, or `null` when the route is unknown.
 */
export function renderInternal(url: string, tabId: TabId): ReactElement | null {
  const route = routeOf(url);
  return route ? createElement(route.page, { url, tabId }) : null;
}

/**
 * The name an internal page goes by, for its tab.
 *
 * Internal pages are drawn by the chrome, not loaded as documents, so no page
 * title ever reaches Rust and the tab would otherwise be labelled with its URL.
 *
 * @param url - Any tab URL.
 * @returns The page's translated name, or `null` when it is not a known internal page.
 */
export function internalTitle(url: string): string | null {
  const route = routeOf(url);
  return route ? t(route.title) : null;
}

function routeOf(url: string) {
  // The host names the page; a path, query or fragment after it belongs to the page.
  const name = url.slice(INTERNAL_PREFIX.length).split(/[/?#]/, 1)[0] ?? "";
  // Own keys only: `haku://constructor` must not resolve to Object's prototype.
  return isInternalUrl(url) && Object.hasOwn(routes, name) ? routes[name] : undefined;
}
