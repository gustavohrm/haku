/**
 * How the address field presents a URL while it is not being edited.
 *
 * The secure scheme and a bare trailing slash are noise at rest, so they are
 * dropped the way other browsers drop them. Anything that changes what the
 * URL means, such as `http:` or a path, is kept. Editing always shows the
 * URL in full.
 */

const SECURE_SCHEME = "https://";

/**
 * @param url - The URL the tab is showing.
 * @returns The text to display at rest.
 */
export function displayAddress(url: string): string {
  if (!url.startsWith(SECURE_SCHEME)) {
    return url;
  }
  const rest = url.slice(SECURE_SCHEME.length);
  return rest.endsWith("/") && rest.indexOf("/") === rest.length - 1 ? rest.slice(0, -1) : rest;
}

/**
 * The icon leading the address field.
 *
 * It states what the connection is, and turns into a search glyph while the
 * field is being typed into, since what is typed may be a search.
 *
 * @param url - The URL the tab is showing.
 * @param editing - Whether the field has focus.
 * @returns An icon class.
 */
export function addressIcon(url: string, editing: boolean): string {
  if (editing || url === "") {
    return "ic-search";
  }
  if (url.startsWith(SECURE_SCHEME)) {
    return "ic-lock";
  }
  if (url.startsWith("http://")) {
    return "ic-lock-open";
  }
  return "ic-globe";
}
