import { type ExtensionStore } from "@bindings";

/**
 * The stores' addresses, as far as the toolbar needs them.
 * [Extensions § From the stores](../../../docs/specs/extensions.md#from-the-stores).
 */

/** An extension id: 32 letters from `a` to `p`, as Rust checks again. */
const EXTENSION_ID = /^[a-p]{32}$/;

/** An extension a store's page is about. */
export type StoreExtension = { store: ExtensionStore; id: string };

/**
 * The extension a store's page is about, for **Add to Haku**.
 *
 * Detail pages are `https://chromewebstore.google.com/detail/<name>/<id>` in
 * the Chrome Web Store, and the store's old address,
 * `https://chrome.google.com/webstore/detail/…`, which redirects there; and
 * `https://microsoftedge.microsoft.com/addons/detail/<name>/<id>` in
 * Microsoft Edge Add-ons. Either may leave out the name, and the pages under
 * them, such as reviews, count too.
 *
 * @param url - The address of the page the tab shows.
 * @returns The store and the extension's id, or `null` for any other page.
 */
export function storeExtension(url: string): StoreExtension | null {
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
  let store: ExtensionStore;
  let detail: string[];
  if (parsed.hostname === "chromewebstore.google.com" && segments[0] === "detail") {
    [store, detail] = ["chrome", segments.slice(1)];
  } else if (parsed.hostname === "chrome.google.com" && segments[0] === "webstore" && segments[1] === "detail") {
    [store, detail] = ["chrome", segments.slice(2)];
  } else if (
    parsed.hostname === "microsoftedge.microsoft.com" &&
    segments[0] === "addons" &&
    segments[1] === "detail"
  ) {
    [store, detail] = ["edge", segments.slice(2)];
  } else {
    return null;
  }
  // The id follows the name, or `detail` when there is none; a name can look
  // like an id, so the place after it is tried first.
  const id = [detail[1], detail[0]].find((segment) => segment !== undefined && EXTENSION_ID.test(segment));
  return id === undefined ? null : { store, id };
}
