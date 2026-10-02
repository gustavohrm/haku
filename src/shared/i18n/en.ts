/**
 * The English dictionary, and the shape every other locale must match.
 *
 * Adding Portuguese and Spanish later means adding sibling files typed against
 * this one; a missing or extra key becomes a type error rather than a string
 * that silently falls back at runtime.
 */
export const en = {
  "tabs.list": "Open tabs",
  "tabs.new": "New tab",
  "tabs.close": "Close tab",
  "tabs.untitled": "Untitled",
  "tabs.discarded": "Discarded — reloads when you open it",
  "tabs.fixed": "Kept loaded",

  "toolbar.navigation": "Navigation",
  "toolbar.back": "Back",
  "toolbar.forward": "Forward",
  "toolbar.reload": "Reload",
  "toolbar.address": "Search or enter address",
  "toolbar.devtools": "Open developer tools",
  "toolbar.settings": "Settings",

  "site.menu": "Site information",
  "site.secure": "Connection is secure",
  "site.insecure": "Connection is not secure",
  "site.keepLoaded": "Keep this tab loaded",
  "site.keepLoaded.warning":
    "Not recommended. This tab stays in memory and keeps running in the background, whatever the optimization settings say. Use it only for pages the automatic behavior does not suit.",

  "window.minimize": "Minimize",
  "window.maximize": "Maximize",
  "window.close": "Close",

  "settings.title": "Settings",
  "settings.appearance": "Appearance",
  "settings.theme": "Theme",
  "settings.theme.system": "Match system",
  "settings.theme.light": "Light",
  "settings.theme.dark": "Dark",
  "settings.performance": "Performance",
  "settings.capacity": "Loaded tabs",
  "settings.capacity.help":
    "How many tabs stay loaded at once. Others are discarded and reload when you return to them.",
  "settings.browsing": "Browsing",
  "settings.search": "Search engine URL",
  "settings.home": "Home page",

  "newTab.title": "New tab",
  "newTab.prompt": "Type an address or a search above.",

  "history.title": "History",
  "history.empty": "Nothing here yet.",
  "history.clear": "Clear history",

  "error.title": "Something went wrong",

  "feedback.dismiss": "Dismiss notification",

  "dialog.from": "{host} says",
  "dialog.fromPage": "This page says",
  "dialog.ok": "OK",
  "dialog.cancel": "Cancel",
  "dialog.leave.title": "Leave site?",
  "dialog.leave.message": "Changes you made may not be saved.",
  "dialog.leave.confirm": "Leave",
  "dialog.leave.cancel": "Stay",
} as const;

export type TranslationKey = keyof typeof en;
export type Dictionary = Record<TranslationKey, string>;
