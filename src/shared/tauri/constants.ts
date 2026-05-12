export const COMMANDS = {
  ENSURE_BROWSER_WEBVIEW: "ensure_browser_webview",
  SET_BROWSER_BOUNDS: "set_browser_bounds",
  NAVIGATE_BROWSER: "navigate_browser",
  RELOAD_BROWSER: "reload_browser",
  OPEN_BROWSER_DEVTOOLS: "open_browser_devtools",
  SCROLL_BROWSER_TO: "scroll_browser_to",
} as const;

export const EVENTS = {
  TAB_INFO: "tab-info",
  SCROLL_POSITION: "scroll-position",
} as const;
