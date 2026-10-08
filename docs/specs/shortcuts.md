---
status: IMPLEMENTED
last_updated: 2026-10-08
scope: Browser keyboard shortcuts, how they are read wherever focus is, and reopening closed tabs.
---

# Shortcuts

## How keys are read

[First release § Keyboard input](../first-release.md#keyboard-input-while-a-page-has-focus) settles the mechanism: keys are intercepted natively, through WebView2's `AcceleratorKeyPressed`, and there is one keymap, in Rust (`model::keymap`).

The handler is attached to the chrome and to every content webview, so a shortcut does the same thing wherever focus is. A key the keymap binds is marked handled before the webview acts on it: neither the page nor the engine's own handling of the key sees it, so a page cannot override Ctrl+W. A key the keymap does not bind passes through untouched, which is what leaves copy, paste, find, print and zoom to the page and the engine.

A held key repeats. Only the first press runs its shortcut; repeats are still kept from the page. Otherwise holding Ctrl+T would open a tab per repeat.

Shortcuts run off the UI thread, like commands, because most of them drive webviews. They act on the active tab of the window they were pressed in. In a popup, which shows one page, a shortcut that opens a tab opens it in the browser window used last ([Windows and popups](new-windows.md#windows)).

## The keymap

Bindings follow Chrome's, because that is what users' hands already know. Modifiers match exactly: Ctrl+Shift+W is not Ctrl+W.

| Keys                        | Does                                 |
| --------------------------- | ------------------------------------ |
| Ctrl+T                      | Open a new tab, in the address field |
| Ctrl+N                      | Open a new browser window            |
| Ctrl+W, Ctrl+F4             | Close the tab                        |
| Ctrl+Shift+T                | Reopen the last closed tab           |
| Ctrl+Tab, Ctrl+PageDown     | Next tab, wrapping                   |
| Ctrl+Shift+Tab, Ctrl+PageUp | Previous tab, wrapping               |
| Ctrl+1 … Ctrl+8             | The tab at that position             |
| Ctrl+9                      | The last tab                         |
| Ctrl+L, Alt+D, F6           | Focus the address field              |
| Ctrl+R, F5                  | Reload                               |
| Alt+Left, Alt+Right         | Back, forward                        |
| F12, Ctrl+Shift+I           | Developer tools for the tab's page   |

The keymap is fixed. Making it configurable is a settings format, and nothing needs it yet.

## Focusing the address field

Focusing the address field from a page first moves keyboard focus to the chrome webview of the tab's window, then emits `AddressFocusRequested` to that chrome alone, with the tab it is for. The event names the tab because Ctrl+T requests focus for a tab the interface has not rendered yet. The interface focuses the field when that tab is active, and holds the request until it is.

## Reopening closed tabs

Closing a tab remembers it, with its position, its whole back and forward history, its scroll and the form contents Rust holds for it ([Tab optimization](tab-optimization.md)). Ctrl+Shift+T reopens the most recent one under a new id, in the window it was closed in, or in the browser window used last if that one has closed. A web page comes back discarded, so it loads into whatever slot it gets and its scroll and drafts are put back as for any discarded tab; an internal page is drawn by the chrome again.

The position is the index the tab had when it closed, clamped to the strip as it is now. That is exact when nothing else changed in between; after other tabs were opened or closed, the tab lands near where it was rather than exactly there.

- The last **25** closed tabs are remembered (`CLOSED_LIMIT`). Each holds no webview, only Rust's record of the tab: its history, scroll, drafts and page reading. That is small, so the limit is about what is useful, not memory.
- They are kept **in memory only**, not in the session. Reopening across a restart would make the session format carry them; nothing asked for it yet.
- A tab that never left an internal page, such as a new tab opened and closed again, is not remembered. Reopening it would bring back nothing, and it would shadow the tab the user meant.
