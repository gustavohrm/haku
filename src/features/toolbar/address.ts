/**
 * How the address field presents a URL while it is not being edited.
 *
 * The secure scheme and a bare trailing slash are noise at rest, so they are
 * dropped the way other browsers drop them. Anything that changes what the
 * URL means, such as `http:` or a path, is kept. Editing always shows the
 * URL in full. The new-tab page has no address worth showing, so the field
 * stays empty there, on its placeholder.
 */

import { isNewTab } from "@app/router";

const SECURE_SCHEME = "https://";

/**
 * @param url - The URL the tab is showing.
 * @returns The text to display at rest.
 */
export function displayAddress(url: string): string {
  if (isNewTab(url)) {
    return "";
  }
  if (!url.startsWith(SECURE_SCHEME)) {
    return url;
  }
  const rest = url.slice(SECURE_SCHEME.length);
  return rest.endsWith("/") && rest.indexOf("/") === rest.length - 1 ? rest.slice(0, -1) : rest;
}

/**
 * The text the address field starts from when it is focused: the URL in full,
 * or nothing on the new-tab page, so typing does not begin by deleting it.
 *
 * @param url - The URL the tab is showing.
 */
export function editableAddress(url: string): string {
  return isNewTab(url) ? "" : url;
}

/**
 * Whether a page is served over plain HTTP, which the address field warns
 * about: anyone on the network can read and change it.
 *
 * @param url - The URL the tab is showing.
 */
export function isInsecure(url: string): boolean {
  return url.startsWith("http://");
}

/**
 * The icon stating what a popup's connection is.
 *
 * A popup's bar is read-only, with no site menu behind its icon, so the icon
 * is the only place the connection is stated.
 *
 * @param url - The URL the popup is showing.
 * @returns An icon class.
 */
export function connectionIcon(url: string): string {
  if (url.startsWith(SECURE_SCHEME)) {
    return "ic-lock";
  }
  if (isInsecure(url)) {
    return "ic-lock-open";
  }
  return "ic-globe";
}
