---
status: IMPLEMENTED
last_updated: 2026-10-08
scope: How unpacked Chrome extensions are installed, switched on and off, and reached.
---

# Extensions

[First release § Extensions](../first-release.md#extensions) settles the approach: no store, unpacked Chrome extensions through WebView2's own extension support, at the user's own risk. This covers the second tier, Chrome extensions. User scripts and styles are not built.

## Installing

Each subfolder of `extensions/` in the application data directory (`%APPDATA%\com.hakubrowser.app\extensions` on Windows) is one unpacked extension. At startup, off the UI thread, Haku:

1. installs every subfolder into the profile, which every webview shares, so an extension runs in every page;
2. uninstalls any extension the profile holds whose folder is gone;
3. switches each one on or off as the settings say.

The folder is the list. Rejected: a list of paths in the settings, added by typing a path. It lets extensions live anywhere, at the cost of a persisted list and interface to edit it, and nothing needed that yet. Settings has a button that opens the folder.

A folder the engine will not install is listed in settings by name, so a broken or unsupported extension is not silently missing. Changes to the folder take effect on the next start.

An extension's id follows from its folder's path, so moving the folder makes it a different extension, with its own storage.

## Switching on and off

`disabledExtensions` in the settings holds the ids switched off; every other installed extension is on, so a newly added one starts enabled. A switch applies at once, in every page, without a restart: the engine enables and disables an installed extension live.

Rejected: one switch for all extensions. Per-extension switches cover that case one by one, and also comparing the memory of one extension against another.

## The engine is always extension-capable

`AreBrowserExtensionsEnabled` is an option of the WebView2 environment, fixed when it is created, and every webview sharing a profile must agree on it: a content webview created with a different value fails to be created. The chrome window therefore sets `browserExtensionsEnabled` in `tauri.conf.json`, and every content webview sets it on its builder. It is never turned off, because it cannot change while Haku runs; with no extension installed it loads nothing.

## Popups

The engine runs extensions but draws no toolbar, so an extension's toolbar button and its popup do not exist. Until Haku draws them, settings lists an **Open** button for each extension with a popup (`action.default_popup` in its manifest), which opens the popup page as a tab, and the address field accepts `chrome-extension://` addresses.

An extension that acts on "the current tab" from its popup sees the popup's own tab there, so such actions do not work from it. In-page features do: Bitwarden's inline autofill menu and passkeys work, and uBlock Origin Lite blocks requests.

## Service workers

An extension's background worker is exempt from stopping idle workers; see [Webview pool § Service workers](webview-pool.md#service-workers).

## Measured compatibility

On the WebView2 runtime 154.0.4258.62:

| Extension                    | Result                                                                                   |
| ---------------------------- | ---------------------------------------------------------------------------------------- |
| uBlock Origin Lite 2026.1006 | Blocks requests.                                                                         |
| Bitwarden 2026.9.3           | Logs in from its popup page. Inline autofill and passkeys work, Google sign-in included. |
| uBlock Origin (Manifest V2)  | Not tried. Chromium removed Manifest V2, so it is not expected to load.                  |
