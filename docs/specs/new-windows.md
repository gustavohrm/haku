---
status: IMPLEMENTED
last_updated: 2026-10-08
scope: Browser windows and popups, what happens when a page asks for a new window, by a link or by script, and when it asks to close one.
---

# Windows and popups

Haku has any number of browser windows, sharing one webview pool as [First release § One pool across windows](../first-release.md#one-pool-across-windows) decided. A page's popup opens in a window of its own, still connected to the page that opened it.

## Windows

There are two kinds of window.

- A **browser window** has a tab strip and an address field. Ctrl+N opens one on the home page, and so does Shift+clicking a link.
- A **popup** shows the one page that opened it, under a bar with that page's address, read-only, and the window controls. It never takes another tab: a tab opened from a popup, by a link or by a shortcut, opens in the browser window used last, which is brought to the front.

Every tab belongs to one window. Each window shows its own active tab, so every window's visible tab is protected as [Webview pool § Capacity](webview-pool.md#capacity) protects the active one, and capacity grows with the number of windows rather than starving one of them. A webview stays in the window it was last used in. When a tab in another window takes it, it is moved there with `Webview::reparent`, and that window's chrome is raised over it, as when a webview is created.

Rust creates the windows; `tauri.conf.json` declares none. A window and its chrome webview share the label `window-<id>`, which is also how a command knows which window called it: commands act for the window whose chrome called them, and each chrome receives only its own window's `StateChanged`. The shared work of the profile, such as installing extensions and watching service workers, runs on the chrome of the window opened first among those still open.

The window used last is the one most recently focused. A tab opened from nowhere in particular, such as by a popup's shortcut, opens there.

## The request

WebView2 raises `NewWindowRequested` for `target="_blank"` links, `window.open`, and the engine's own "open in new tab". `platform/` takes a deferral on it, so the page waits, and reports `PageSignal::WindowRequested`. The request is then answered once, either by handing it a webview or by refusing it, and the engine never opens a window of its own.

| The request                                         | What opens                                                                                                                       |
| --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| Made by the page on its own, without a click or key | Nothing, as other browsers block it                                                                                              |
| For a `haku://` page                                | Nothing: a page never opens Haku's own pages                                                                                     |
| A popup: asked for with a size or position          | A popup window, connected to the page                                                                                            |
| For anything but an `http` or `https` address       | The active tab of the page's window, connected to the page, since only the engine can load a `blob:` or blank page on its behalf |
| A web address, with Shift held but not Ctrl         | A new browser window                                                                                                             |
| A web address, with Ctrl held but not Shift         | A tab left for later, loaded when it is selected                                                                                 |
| A web address otherwise                             | The active tab                                                                                                                   |

A click counts for about five seconds: Chromium lets a page act on one for that long, so a request made a moment after the click is not blocked.

Ctrl+Shift opens a web address as the active tab, as in Chrome.

Requests are answered one at a time, in the order they were made. A tab opened from a page goes right after it, and after the tabs that page opened before, so links opened for later line up in the order they were clicked.

Middle-clicking a link opens it as the active tab. The engine does not say which button was used, and by the time the request arrives the button is already up.

### Where a popup goes

A popup's window is put where the page asked, in CSS pixels, with its page at the size the page asked for and the window taller by its bar. A popup that asks for no position is centred, and one that asks for no size gets a page of 500 by 600. It can be resized, down to 240 by 160.

## Connected pages

A page that opens a sign-in popup waits for it to report back through `window.opener`, so a popup, or a tab opened for a page that only the engine can load, is given a webview handed to the request, which keeps that connection.

That webview has never loaded anything: the engine only accepts one in that state for the request. It is therefore never a parked webview or one taken from another tab. The pool grows by one for it and is trimmed back to capacity as eviction would, parked webviews first. If every other webview is protected, the page gets none: the request is refused and the popup loads its address like any other page, unconnected.

While a connected page is open, the tab that opened it [must run](webview-pool.md#must-run), whatever the optimization policies say, _Discard: always_ included, so the page a sign-in reports back to is still there. A connected tab that loses its page, by being discarded or evicted, loses its connection with it, and its opener no longer has to run. A link opened in a new browser window is not connected.

## Closing

A page opened by script may close itself with `window.close()`, as a sign-in popup does when it is done. WebView2 raises `WindowCloseRequested`, and its tab is closed. Closing a popup's tab closes the popup. Closing a tab a page opened, by any means, returns to the tab that opened it if that tab is still open in the same window, rather than to the tab on its right.

Closing a browser window closes its tabs, which are not remembered for reopening, and destroys the webviews in it. A browser window's last tab closing opens the home page in its place, as it always has; the window stays.

Closing the last browser window quits Haku, popups included, and keeps that window's tabs for the next launch.

## The session

The session stores each browser window, in the order they were last used, with its tabs, its active tab and where it was on screen. Popups are not stored: the page that opened one reloads on the next launch, and the connection cannot come back. A window that was never moved or resized opens maximized.

A session stored before Haku had windows, a flat list of tabs, is read as one window and written back in the new shape.

## Not built

- Tearing a tab off into its own window, and moving tabs between windows.
- Reopening a closed window.
- A window restored onto a screen that is no longer attached is not moved back onto one.
- A new window's address field is not focused: the window's interface is not listening yet when Ctrl+N asks for it.
- A connected tab left in the background can be discarded like any other, which cuts its connection for good. Reloading cannot restore it.
- There is no indicator for a blocked request.
