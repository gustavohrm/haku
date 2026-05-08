export {
  DEFAULT_TAB_URL,
  createTabState,
  getCurrentWebsite,
  resolveNavigationTarget,
} from "./state";
export type { ManagedTab, TabInfoUpdate, TabState } from "./state";
export {
  ensureBrowserWebview,
  navigateBrowser,
  openBrowserDevtools,
  reloadBrowser,
  setBrowserBounds,
} from "./webview";
export type { BrowserBounds } from "./webview";
