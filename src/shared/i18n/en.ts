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
  "tabs.relieved": "Unloaded to free memory — reloads when you open it",

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
  "site.keepSite": "Don't unload this site",
  "site.keepSite.help":
    "Tabs on this site stay in memory, paused, as if they held unsaved work. Use it for pages that lose something Haku cannot see when they reload.",

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
  "settings.preset.help": "Fills in the settings below.",
  "settings.capacity": "Loaded tabs",
  "settings.capacity.help":
    "How many tabs can stay in memory at once, including the one you are viewing. Tabs you keep loaded add to this.",
  "settings.keptMemory": "Background memory (MB)",
  "settings.keptMemory.help":
    "How much memory background tabs may hold when kept for what reloading them would lose, such as where you were in a page. Tabs with unsaved text are kept even beyond it.",
  "settings.freeze": "Freeze background tabs",
  "settings.freeze.help":
    "A frozen tab is paused and gives back some memory. It opens again instantly, without reloading. Smart leaves tabs playing audio or using the camera, microphone or screen running.",
  "settings.discard": "Discard background tabs",
  "settings.discard.help":
    "A discarded tab frees all its memory and reloads when you open it. Smart discards a tab a minute after you leave it unless reloading would lose something, and sooner when memory runs low. Never means Haku never discards on its own: a tab still reloads when every loaded tab is in use.",
  "settings.policy.never": "Never",
  "settings.policy.smart": "Smart",
  "settings.policy.always": "Always",
  "settings.disabledByDiscard": "Has no effect while background tabs are always discarded.",
  "settings.browsing": "Browsing",
  "settings.search": "Search engine URL",
  "settings.home": "Home page",
  "settings.extensions": "Extensions",
  "settings.extensions.empty": "No extensions installed.",
  "settings.extensions.open": "Open",
  "settings.extensions.failed": "Could not install:",
  "settings.extensions.error": "Something went wrong with extensions:",
  "settings.extensions.folder": "Open extensions folder",
  "settings.extensions.help":
    "Put each unpacked Chrome extension in its own folder there, then restart Haku. Switch an extension off to stop it. Extensions are installed at your own risk.",

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
  "memory.position": "Why loaded",
  "memory.position.visible": "Visible",
  "memory.position.mustRun": "Must run",
  "memory.position.kept": "Kept",
  "memory.budget": "Background memory used",
  "memory.memory": "Memory",
  "memory.loss": "Discarding loses",
  "memory.loss.none": "Nothing",
  "memory.loss.state": "State",
  "memory.loss.work": "Work",
  "memory.signal.keptSite": "kept site",
  "memory.signal.unsaved": "unsaved text",
  "memory.signal.unloadArmed": "asks before leaving",
  "memory.signal.formResult": "form result",
  "memory.signal.interactions": "interactions",
  "memory.signal.mediaPaused": "paused media",
  "memory.signal.unreadable": "could not be read",
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
