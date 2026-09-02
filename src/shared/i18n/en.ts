/**
 * The English dictionary, and the shape every other locale must match.
 *
 * Adding Portuguese and Spanish later means adding sibling files typed against
 * this one; a missing or extra key becomes a type error rather than a string
 * that silently falls back at runtime.
 */
export const en = {
  "tabs.new": "New tab",
  "tabs.close": "Close tab",
  "tabs.pin": "Keep this tab loaded",
  "tabs.unpin": "Stop keeping this tab loaded",
  "tabs.untitled": "Untitled",
  "tabs.suspended": "Suspended — reloads when you open it",

  "toolbar.back": "Back",
  "toolbar.forward": "Forward",
  "toolbar.reload": "Reload",
  "toolbar.address": "Search or enter address",
  "toolbar.devtools": "Open developer tools",
  "toolbar.settings": "Settings",

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
    "How many tabs stay loaded at once. Others are suspended and reload when you return to them.",
  "settings.search": "Search engine URL",
  "settings.home": "Home page",

  "newTab.title": "New tab",
  "newTab.prompt": "Type an address or a search above.",

  "history.title": "History",
  "history.empty": "Nothing here yet.",
  "history.clear": "Clear history",

  "error.title": "Something went wrong",
} as const;

export type TranslationKey = keyof typeof en;
export type Dictionary = Record<TranslationKey, string>;
