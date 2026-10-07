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
  "tabs.frozen": "Frozen — resumes instantly when you open it",
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
  "site.warning": "Warning",
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
  "settings.optimization": "Optimization",
  "settings.preset": "Preset",
  "settings.preset.saveMemory": "Save memory",
  "settings.preset.balanced": "Balanced",
  "settings.preset.performance": "Performance",
  "settings.preset.custom": "Custom",
  "settings.preset.help": "Fills in the settings below. Balanced and Performance size them for this computer's memory.",
  "settings.capacity": "Loaded tabs",
  "settings.capacity.help":
    "How many tabs can stay in memory at once, including the one you are viewing. Tabs you keep loaded add to this.",
  "settings.freeze": "Freeze background tabs",
  "settings.freeze.help":
    "A frozen tab is paused and gives back some memory. It opens again instantly, without reloading. Smart leaves tabs playing audio running.",
  "settings.discard": "Discard background tabs",
  "settings.discard.help":
    "A discarded tab frees all its memory and reloads when you open it. Smart discards tabs you have not opened in a while, sooner when memory runs low. Never means Haku never discards on its own: a tab still reloads when every loaded tab is in use.",
  "settings.policy.never": "Never",
  "settings.policy.smart": "Smart",
  "settings.policy.always": "Always",
  "settings.disabledByDiscard": "Has no effect while background tabs are always discarded.",
  "settings.browsing": "Browsing",
  "settings.search": "Search engine URL",
  "settings.home": "Home page",

  "newTab.title": "New tab",
  "newTab.prompt": "Type an address or a search above.",

  "history.title": "History",
  "history.empty": "Nothing here yet.",
  "history.clear": "Clear history",

  "memory.title": "Memory",
  "memory.pressure": "Memory pressure",
  "memory.pressure.normal": "Normal",
  "memory.pressure.tight": "Tight",
  "memory.pressure.critical": "Critical",
  "memory.headroom": "Headroom",
  "memory.slotsTotal": "Held by loaded tabs",
  "memory.unattributedTotal": "Held by the engine",
  "memory.slots": "Loaded tabs",
  "memory.slot": "Slot",
  "memory.tab": "Tab",
  "memory.state": "State",
  "memory.state.live": "Running",
  "memory.state.frozen": "Frozen",
  "memory.state.parked": "Parked",
  "memory.memory": "Memory",
  "memory.unattributed": "Engine processes",
  "memory.process": "Process",
  "memory.pid": "PID",
  "memory.process.browser": "Browser",
  "memory.process.renderer": "Renderer",
  "memory.process.gpu": "GPU",
  "memory.process.utility": "Utility",
  "memory.process.other": "Other",

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
