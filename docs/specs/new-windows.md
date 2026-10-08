---
status: APPROVED
last_updated: 2026-10-08
scope: What happens when a page asks for a new window, by a link or by script, and when it asks to close one.
---

# New windows and popups

Haku does not open new windows yet. Until it does, every window a page asks for, popups included, opens in a tab of the one window Haku has. That is a stopgap, not the design: opening real windows, popups first, is the [next step](#next-step).

What the stopgap keeps is the connection a popup needs. A page that opens a sign-in popup waits for the popup to report back through `window.opener`, so the tab such a request opens in gets a webview handed to the request, which keeps that connection.

## The request

WebView2 raises `NewWindowRequested` for `target="_blank"` links, `window.open`, and the engine's own "open in new tab". `platform/` takes a deferral on it, so the page waits, and reports `PageSignal::WindowRequested`. The request is then answered once, either by handing it a webview or by refusing it, and the engine never opens a window of its own.

| The request                                         | What opens                                                                                                  |
| --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Made by the page on its own, without a click or key | Nothing, as other browsers block it                                                                         |
| For a `haku://` page                                | Nothing: a page never opens Haku's own pages                                                                |
| A popup: asked for with a size or position          | The active tab, connected to the page                                                                       |
| For anything but an `http` or `https` address       | The active tab, connected to the page, since only the engine can load a `blob:` or blank page on its behalf |
| A web address, with Ctrl held                       | A tab left for later, loaded when it is selected                                                            |
| A web address otherwise                             | The active tab                                                                                              |

A click counts for about five seconds: Chromium lets a page act on one for that long, so a request made a moment after the click is not blocked.

A tab opened from a page goes right after it, and after the tabs that page opened before, so links opened for later line up in the order they were clicked.

Middle-clicking a link opens it as the active tab. The engine does not say which button was used, and by the time the request arrives the button is already up.

## Connected tabs

A connected tab is the active tab, in a webview that has never loaded anything: the engine only accepts one in that state for the request. It is therefore never a parked webview or one taken from another tab. The pool grows by one for it and is trimmed back to capacity as eviction would, parked webviews first. If every other webview is protected, the tab gets none: the request is refused and the tab loads its address like any other, unconnected.

While a connected tab is open, the tab that opened it [must run](webview-pool.md#must-run), whatever the optimization policies say, so the page a sign-in reports back to is still there.

## Closing

A page opened by script may close itself with `window.close()`, as a sign-in popup does when it is done. WebView2 raises `WindowCloseRequested`, and its tab is closed. Closing a tab a page opened, by any means, returns to the tab that opened it if that tab is still open, rather than to the tab on its right.

## Next step

Real windows. A popup opens in a window of its own, at the size and position it asked for, and a request for a new window that is not a popup, such as Shift+click, opens a new browser window. Both depend on the multi-window work in [First release § One pool across windows](../first-release.md#one-pool-across-windows): a window that shows pooled webviews and its own chrome.

## Not built

- Real popup windows and new browser windows; see [Next step](#next-step).
- A connected tab left in the background can be discarded like any other, which cuts its connection for good. Reloading cannot restore it.
- There is no indicator for a blocked request.
