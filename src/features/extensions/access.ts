import { type ExtensionAccess } from "@bindings";
import { t, type TranslationKey } from "@shared/i18n";

/** How many sites are named before the rest are counted. */
const SITES_NAMED = 3;

const SENTENCES: Record<Exclude<ExtensionAccess["kind"], "sites">, TranslationKey> = {
  allSites: "extensions.access.allSites",
  changeHistory: "extensions.access.changeHistory",
  readHistory: "extensions.access.readHistory",
  topSites: "extensions.access.topSites",
  bookmarks: "extensions.access.bookmarks",
  downloads: "extensions.access.downloads",
  browsingData: "extensions.access.browsingData",
  readClipboard: "extensions.access.readClipboard",
  writeClipboard: "extensions.access.writeClipboard",
  blockContent: "extensions.access.blockContent",
  capture: "extensions.access.capture",
  location: "extensions.access.location",
  notifications: "extensions.access.notifications",
  email: "extensions.access.email",
  privacy: "extensions.access.privacy",
  contentSettings: "extensions.access.contentSettings",
  management: "extensions.access.management",
  nativeMessaging: "extensions.access.nativeMessaging",
  debugger: "extensions.access.debugger",
};

/**
 * What an extension may do, as the sentence shown before it is added.
 * [Extensions § Before installing](../../../docs/specs/extensions.md#before-installing).
 *
 * @param access - One kind of access its manifest asks for.
 */
export function describeAccess(access: ExtensionAccess): string {
  if (access.kind !== "sites") {
    return t(SENTENCES[access.kind]);
  }
  const named = access.hosts.slice(0, SITES_NAMED).join(", ");
  const rest = access.hosts.length - SITES_NAMED;
  const sites =
    rest > 0
      ? t(rest === 1 ? "extensions.access.sites.more.one" : "extensions.access.sites.more.many")
          .replace("{sites}", () => named)
          .replace("{count}", String(rest))
      : named;
  return t("extensions.access.sites").replace("{sites}", () => sites);
}
