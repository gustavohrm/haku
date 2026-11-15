/**
 * The Chrome Web Store's addresses, as far as the toolbar needs them.
 * [Extensions § From the Chrome Web Store](../../../docs/specs/extensions.md#from-the-chrome-web-store).
 */

/** An extension id: 32 letters from `a` to `p`, as Rust checks again. */
const EXTENSION_ID = /^[a-p]{32}$/;

/**
 * The extension a Chrome Web Store page is about, for **Add to Haku**.
 *
 * Its detail pages are `https://chromewebstore.google.com/detail/<name>/<id>`,
 * with or without the name, the pages under them, such as its reviews, and
 * the store's old address,
 * `https://chrome.google.com/webstore/detail/…`, which redirects there.
 *
 * @param url - The address of the page the tab shows.
 * @returns The extension's id, or `null` for any other page.
 */
export function storeExtensionId(url: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.protocol !== "https:") {
    return null;
  }
  const segments = parsed.pathname.split("/").filter((segment) => segment !== "");
  let detail: string[];
  if (parsed.hostname === "chromewebstore.google.com" && segments[0] === "detail") {
    detail = segments.slice(1);
  } else if (parsed.hostname === "chrome.google.com" && segments[0] === "webstore" && segments[1] === "detail") {
    detail = segments.slice(2);
  } else {
    return null;
  }
  // The id follows the name, or `detail` when there is none; a name can look
  // like an id, so the place after it is tried first.
  return [detail[1], detail[0]].find((segment) => segment !== undefined && EXTENSION_ID.test(segment)) ?? null;
}
