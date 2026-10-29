---
status: IMPLEMENTED
last_updated: 2026-10-29
scope: How unpacked Chrome extensions are installed and uninstalled, switched on and off, pinned, and opened from the toolbar.
---

# Extensions

[First release § Extensions](../first-release.md#extensions) settles the approach: no store, unpacked Chrome extensions through WebView2's own extension support, at the user's own risk. This covers the second tier, Chrome extensions. User scripts and styles are not built.

## Installing

Each subfolder of `extensions/` in the application data directory (`%APPDATA%\com.hakubrowser.app\extensions` on Windows) is one unpacked extension. At startup, off the UI thread, Haku:

1. installs every subfolder into the profile, which every webview shares, so an extension runs in every page;
2. switches each one on or off as the settings say.

The folder is the list. Rejected: a list of paths in the settings, added by typing a path. It lets extensions live anywhere, at the cost of a persisted list and interface to edit it, and nothing needed that yet. Settings has a button that opens the folder.

A folder the engine will not install is listed in settings by name, and any other failure is shown there as a typed `HakuError`, as [Errors](errors.md) requires, so a broken or unsupported extension is not silently missing. Folders are installed in name order, so the list does not shuffle between launches. Changes to the folder take effect on the next start.

Each extension's toolbar icon is read from its manifest at install, `action.default_icon` or else `icons`, the smallest at least 32 pixels, and reaches the interface as a `data:` URL in `Extension.icon`. The engine does not report the icon an extension sets while it runs.

Until the folder is installed, a tab sent to a `chrome-extension://` page, such as one restored from the last session, waits on the blank page and loads once installing ends. Loaded earlier, the page would fail, and the engine's error page would replace it in the tab's history ([gustavohrm/haku#19](https://github.com/gustavohrm/haku/issues/19)).

An unpacked extension's id follows from its folder's path unless its manifest declares a `key`, so moving a folder without one makes it a different extension, with its own storage.

### Uninstalling

Deleting an extension's folder uninstalls it at the next start, with what it stored.

The engine's list of installed extensions (`GetBrowserExtensions`) includes its own built-in ones, the Edge PDF viewer and the Microsoft Clipboard extension, and gives only an id and a name, so nothing in it tells them apart from the user's. Treating every listed extension without a folder as deleted tried to remove both, and the engine refused with `E_FAIL`. Haku therefore remembers what it installed: `extensions.json`, next to `settings.json`, records each extension's id and the folder it came from, and is rewritten once the extensions have been installed and removed at startup. An extension installed by a start that ends before then is not recorded, and deleting its folder later leaves it installed. It is internal state, not a setting.

At startup, an extension recorded there whose folder no longer exists is removed from the engine, unless the same id was installed again from another folder, as a manifest `key` allows. One whose folder is still there stays installed even if it failed to install this time, so a broken update does not wipe what the extension stored. A removal that fails stays recorded, is retried at the next start, and is listed in settings as a failure.

An extension installed before Haku recorded them is not in the file, so deleting its folder leaves it installed and switchable.

## Switching on and off

`disabledExtensions` in the settings holds the ids switched off; every other installed extension is on, so a newly added one starts enabled. A switch applies at once, in every page, without a restart: the engine enables and disables an installed extension live.

Rejected: one switch for all extensions. Per-extension switches cover that case one by one, and also comparing the memory of one extension against another.

## The engine is always extension-capable

`AreBrowserExtensionsEnabled` is an option of the WebView2 environment, fixed when it is created, and every webview sharing a profile must agree on it: a content webview created with a different value fails to be created. Every window's chrome therefore sets it on its builder in `webview::build_window`, and every content webview sets it on its own. It is never turned off, because it cannot change while Haku runs; with no extension installed it loads nothing.

## The toolbar

Right of the address field, before the developer tools button, the toolbar has an **Extensions** button, a puzzle piece, and to its left a button for each pinned extension, as in Chrome.

The Extensions button opens a menu of every extension switched on, each with its icon, its name and a pin toggle, shown on a pinned extension and on hover or focus otherwise, and **Manage extensions** at the bottom, which opens settings at its Extensions section, `haku://settings/extensions`. Clicking an extension opens its popup. One without a popup (`action.default_popup` in its manifest) is listed but cannot be opened. The menu closes as the site menu does: on a click outside it, Escape, or the window losing focus or resizing. Settings keeps the switches and no longer opens popups as tabs; the address field still accepts `chrome-extension://` addresses, so any extension page can be opened as one.

`pinnedExtensions` in the settings holds the ids pinned, in the order they were pinned, so a new pin goes last, rightmost. A pinned extension that is switched off or no longer installed shows no button, and its pin is kept, so it comes back with it.

Rejected: a button for every extension, Chrome's old behaviour, which crowds the toolbar with extensions that are never opened from it.

## Popups

An extension's popup opens in a borderless window of its own, owned by the browser window, below the button it was opened from with their right edges lined up: its own button when pinned, the Extensions button otherwise. It is moved as little as it takes to stay inside the screen's work area. The window stays hidden until the page has loaded and laid out within the window's size, for up to three seconds, then is shown sized to the page's content, between 25 by 25 and 800 by 600 as in Chrome; a larger page scrolls. It grows when its page grows, and never shrinks. Until it has grown, the page reads its window's size, `innerWidth` and `innerHeight`, as the size it is being made, its content's size within those limits, as it would in Chrome, whose popups grow with the page's own layout. A page that picks its layout from the window's size as it starts would otherwise pick it from the window's size before the first measurement: Bitwarden picked its 240-pixel layout that way.

It closes when it loses focus, as in other browsers: on a click anywhere else, on switching windows, or when the page calls `window.close()`. Clicking the button of the popup that is open closes it instead of reopening it. That click reaches the button only after the popup has lost focus, so a request to open the popup that closed in the last 400 milliseconds is taken as that click. One popup is open at a time. A page opened from the popup, such as uBlock Origin Lite's dashboard, opens as a tab in the browser window used last, and the popup closes.

The toolbar captures its focused tab and passes it explicitly as `tab: TabId` to `open_extension_popup`, alongside the extension id and anchor rectangle. Rust rejects an unknown tab, a tab from another window, or a tab without a visible viewport assignment through the existing error channel before changing popup state. The popup acts on that captured tab. The engine gives each webview a Chromium window of its own with one tab, the popup's included, so "the current window" in an extension's popup would be the popup itself. Haku asks the engine for the window of the named tab's webview, with `Browser.getWindowForTarget` through the DevTools protocol, and tells the popup. An internal or crashed tab supplies no engine page, preserving the existing absent engine-window behavior. The explicit argument does not change the one-popup-at-a-time application limit or the native popup window.

### Adjustments injected into extension pages

Extensions are written for Chrome's windows and tabs, which WebView2 lays out differently. A script injected into every `chrome-extension://` page Haku hosts adjusts these calls. It does not run in an extension's background worker.

| Call                                                                                                                                                                 | What Haku does                                                                                                                                                                         |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `windows.create` with `type: "popup"` and one address                                                                                                                | Opens a Haku popup with `window.open`, at that size and position, and answers with the engine's window for it. The engine's own window would be one Haku does not manage.              |
| `windows.update` with a position, size or state                                                                                                                      | Ignores those: the engine would move the webview inside Haku's window instead, leaving the page blank.                                                                                 |
| `windows.remove`                                                                                                                                                     | Closes the window's tabs, which closes the Haku tab or popup.                                                                                                                          |
| `runtime.openOptionsPage`                                                                                                                                            | Opens the options page as a tab, where the engine does nothing.                                                                                                                        |
| In a toolbar popup, `tabs.query` for the current or last focused window, and `windows.getCurrent`, `windows.getLastFocused` and `windows.get` for the current window | Answers with the active tab's window, as above.                                                                                                                                        |
| In a toolbar popup, `tabs.getCurrent`                                                                                                                                | Answers with nothing, as Chrome does for a popup, where the engine gives the popup's own tab. Bitwarden takes a tab to mean it runs in one, and lays itself out at the window's width. |

`window.open` is given `noopener`: an extension page opened without it reuses its opener's first, blank window object, and the injected script, which runs once per document, never runs in it.

## Limitations

- An extension's background worker sees the engine's windows and tabs unadjusted. A popup it opens with `windows.create` opens as a tab, and its `windows.update` and `windows.remove` act on the engine's windows, which can leave a page blank. Bitwarden's unlock window, opened from its worker, is one case.
- An extension's badge, the icon it sets while it runs, and its title are not shown.
- An extension without a popup, whose toolbar button raises `action.onClicked`, cannot be clicked: the engine gives no way to raise that event.
- `tabs.update` with `active` and `windows.update` with `focused` do not switch Haku's tab or window.
- uBlock Origin Lite opens a new dashboard tab each time instead of returning to the open one: it looks for one in a window of type `normal`, and every engine window is a popup.

## Service workers

An extension's background worker is exempt from stopping idle workers; see [Webview pool § Service workers](webview-pool.md#service-workers).

## Measured compatibility

On the WebView2 runtime 155.0.4283.62:

| Extension                         | Result                                                                                                                                                                                                                                                                 |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| uBlock Origin Lite 2026.1018.2229 | Blocks requests. Its popup shows and acts on the active tab's site, and its dashboard opens as a tab.                                                                                                                                                                  |
| uBlock Origin Lite 2026.1006.1931 | Blocked popups to `doubleclick.net` and `popads.net` opened by `window.open` and by a `target="_blank"` link, and let the same popups to `example.com` open, as Helium did with the same version ([gustavohrm/haku#22](https://github.com/gustavohrm/haku/issues/22)). |
| Bitwarden 2026.10.0               | Its popup opens at full size, and its pop-out button opens it in a popup window. Logging in was not tried on this runtime; it worked from a tab on 154.0.4258.62.                                                                                                      |
| uBlock Origin (Manifest V2)       | Not tried. Chromium removed Manifest V2, so it is not expected to load.                                                                                                                                                                                                |
