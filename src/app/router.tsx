import { HistoryPage } from "@features/history/history-page";
import { SettingsPage } from "@features/settings/settings-page";
import { NewTabPage } from "@features/viewport/new-tab-page";
import { createElement, type ComponentType, type ReactElement } from "react";

/**
 * Pages Haku renders itself.
 *
 * These live in the chrome rather than in a content webview, which means they
 * cost no webview slot, share the interface's theme and translations, and can
 * talk to Rust directly. The `haku:` scheme is what marks a tab as one of them.
 */
const INTERNAL_PREFIX = "haku:";

// Components, not the result of calling them. Invoking a component as a plain
// function runs its hooks inside the caller's hook sequence, so opening or
// leaving an internal page would change that sequence and React would tear the
// whole tree down.
const routes: Record<string, ComponentType> = {
  "new-tab": NewTabPage,
  settings: SettingsPage,
  history: HistoryPage,
};

export function isInternalUrl(url: string): boolean {
  return url.startsWith(INTERNAL_PREFIX);
}

/**
 * Resolves an internal URL to its page.
 *
 * @param url - A `haku:` URL.
 * @returns The page element, or `null` when the route is unknown.
 */
export function renderInternal(url: string): ReactElement | null {
  const route = routes[url.slice(INTERNAL_PREFIX.length)];
  return route ? createElement(route) : null;
}
