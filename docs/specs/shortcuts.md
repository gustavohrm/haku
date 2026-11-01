---
status: IMPLEMENTED
last_updated: 2026-11-01
scope: Browser keyboard shortcuts, how they are read wherever focus is, rebinding them, and reopening closed tabs.
---

# Shortcuts

## How keys are read

[First release § Keyboard input](../first-release.md#keyboard-input-while-a-page-has-focus) settles the mechanism: keys are intercepted natively, through WebView2's `AcceleratorKeyPressed`, and there is one keymap, in Rust (`model::keymap`).

The handler is attached to the chrome and to every content webview, so a shortcut does the same thing wherever focus is. A key the keymap binds is marked handled before the webview acts on it: neither the page nor the engine's own handling of the key sees it, so a page cannot override Ctrl+W. A key the keymap does not bind passes through untouched, which is what leaves copy, paste and zoom to the page and the engine. Find, print and save are bound, so Haku runs them ([Find, print and save](find-print-save.md)).

A held key repeats. Only the first press runs its shortcut; repeats are still kept from the page. Otherwise holding Ctrl+T would open a tab per repeat.

Shortcuts run off the UI thread, like commands, because most of them drive webviews. They share one queue and run in receipt order, including applying their native effects. A page shortcut captures its source tab, slot and window when received. Close, reload, back/forward, address focus and developer tools revalidate that the slot still belongs to that tab and that the tab is still assigned to a visible viewport in that window before acting; a stale request is dropped. Validation and dispatch share the serialized change turn, so another change cannot reuse the slot between them. Tab operations name their tab explicitly.

Strip-selection shortcuts (Ctrl+Tab, Ctrl+PageUp/Down and Ctrl+1–9) instead resolve the window's focused viewport when their turn runs, even if an earlier shortcut replaced their source page. Two queued Ctrl+Tabs therefore advance twice. The split toggle, tab moves, home navigation and duplication also resolve the focused viewport in queue order. Chrome shortcuts resolve it in queue order too. Split, move and duplicate do nothing in popups; Ctrl+Shift+W closes the originating window, including a popup. In a popup, which shows one page, a shortcut that opens a new tab opens it in the browser window used last; Ctrl+Shift+T reopens a tab in the window it was closed in while that window is open ([Windows and popups](new-windows.md#windows), [Reopening closed tabs](#reopening-closed-tabs)).

Native content focus is observed through `ICoreWebView2Controller::GotFocus` (`FocusChangedEventHandler` in the locked `webview2-com` 0.39.1). Rust maps the reporting slot to its current window and viewport assignment; unknown, parked and hidden slots are ignored. Showing content uses `SW_SHOWNA` and `SetIsVisible`, separate from explicit focus; raising chrome uses `SWP_NOACTIVATE`. Content slots are created without requesting focus. Debug builds print `[page focus] slot=<id> window=<id> viewport=<id>` for an assigned slot gaining focus. Split-view model and IPC intents now permit two panes; native pointer/Tab focus and ordering with accelerator delivery still require the running-app acceptance checks before adding interface entry points ([Split view](split-view.md#implementation-slices)).

## The keymap

Ctrl+Shift+N opens a private browser window (`NewPrivateWindow`); Ctrl+N opens an ordinary browser window. Tabs opened from a private popup use the private browser window used last. Private tabs never enter the closed-tab stack, so Ctrl+Shift+T can only reopen ordinary tabs, in an ordinary window.

The default bindings follow Chrome's, because that is what users' hands already know. Modifiers match exactly: Ctrl+Shift+W is not Ctrl+W. Every binding can be changed in settings ([Changing bindings](#changing-bindings)); the table lists the defaults.

| Keys                         | Does                                                                            |
| ---------------------------- | ------------------------------------------------------------------------------- |
| Ctrl+T                       | Open a new tab, in the address field                                            |
| Ctrl+N                       | Open a new browser window                                                       |
| Ctrl+Shift+N                 | Open a new private browser window                                               |
| Ctrl+W, Ctrl+F4              | Close the tab                                                                   |
| Ctrl+Shift+T                 | Reopen the last closed tab                                                      |
| Ctrl+Tab, Ctrl+PageDown      | Next tab, wrapping                                                              |
| Ctrl+Shift+Tab, Ctrl+PageUp  | Previous tab, wrapping                                                          |
| Ctrl+1 … Ctrl+8              | The tab at that position, each its own action (`tab1` … `tab8`)                 |
| Ctrl+9                       | The last tab                                                                    |
| Ctrl+L, Alt+D, F6            | Focus the address field                                                         |
| Ctrl+R, F5                   | Reload                                                                          |
| Alt+Left, Alt+Right          | Back, forward                                                                   |
| Alt+Shift+N                  | Toggle split: new right partner and address focus, or separate the focused pair |
| Ctrl+Shift+PageUp / PageDown | Move the focused tab or pair left/right over one strip unit, without wrapping   |
| Ctrl+Shift+W                 | Close the originating window                                                    |
| Alt+Home                     | Navigate the focused tab to the configured home page                            |
| Shift+Esc                    | Open `haku://memory` in a foreground tab                                        |
| Ctrl+Shift+K                 | Duplicate the focused tab's current URL in a foreground tab                     |
| F12, Ctrl+Shift+I            | Developer tools for the tab's page                                              |
| Ctrl+F                       | Find in the focused tab's page                                                  |
| Ctrl+P                       | Print the focused tab's page                                                    |
| Ctrl+S                       | Save the focused tab's page                                                     |

`model::keymap::DEFAULTS` is the table: each action with its default chords, in this order. Adding a shortcut is a `Shortcut` variant, a line in that table, its arm in `ipc/shortcuts.rs`, and its row and title in the interface ([Settings page § Shortcuts](settings-page.md#shortcuts)). Nothing else repeats a chord: the context menus read their hints from the keymap Rust reports.

## Changing bindings

Decided by the maintainer on 2026-11-01.

- **Stored as exceptions.** `Settings.shortcuts` maps an action's id (`Shortcut`, camelCase, such as `closeTab`) to its chords, only for actions the user changed. An empty list leaves the action unbound. An action left out keeps its defaults, so a default changed in a later release reaches everyone who did not change that action. An entry equal to the defaults is dropped when stored.
- **Chords are text**, written `Ctrl+Alt+Shift+Key` in that modifier order, such as `Ctrl+Shift+T` or `Alt+Shift+N`. Key names are the letter, the digit, `F1` … `F24`, `Tab`, `Left`, `Right`, `Up`, `Down`, `PageUp`, `PageDown`, `Home`, `End`, `Insert`, `Delete` and `Esc`. Stored chords are rewritten in that form; one that does not read, or that is refused, is dropped, as is an unknown action. A chord listed under two actions stays with the first in table order. An action whose every stored chord is dropped keeps its defaults, so only a list stored empty unbinds it.
- **The user's binding wins.** A chord the user gave an action is taken from any action that has it by default, for as long as the user's binding stands. Resetting the action gives it back. Recording a chord another action uses names that action and, once confirmed, moves the chord.
- **Refused chords.** A chord must hold Ctrl or Alt, or be a function key, or be Shift+Esc. Anything else would take a key the user types or selects text with away from every page, since bound keys never reach the page. Ctrl, Ctrl+Shift or Ctrl+Alt with A, C, V, X, Y or Z, and Ctrl or Ctrl+Shift with Insert, Delete, Home, End or an arrow key, are refused too, because pages and text fields edit and move by them. So is Alt+F4, which closes the window. Every default is accepted under these rules. Ctrl+Alt with a letter or digit is accepted, at a known cost: on layouts with AltGr, Windows reports AltGr as Ctrl+Alt, so binding such a chord takes the character it types away, such as Polish AltGr+S ("ś") for Ctrl+Alt+S. Refusing them all would refuse chords that type nothing on most layouts, so the user decides; no default is one ([Split view](split-view.md) D6).
- **Recorded natively.** A bound chord never reaches the interface, so the settings page could not read a press of Ctrl+T. `record_shortcut` instead asks Rust for the next chord pressed in the caller's window: it answers with the chord and, when refused, why (`ChordRefusal`). Plain Esc, `cancel_shortcut_recording`, or another recording in the same window answers it with nothing. While a window records, its interface's chords run no shortcut; keys without Ctrl, Alt or a function key still reach the interface, so recording does not trap Tab. Pages are unaffected. A recording left pending when its window's interface reloads takes that window's next chord and ends. The recorded chord's repeats are consumed while it is held, so holding Alt+F4 while recording does not close the window.
- **Read back as a keymap.** `keymap` lists every action, in table order, with its chords now and its defaults. The interface reads it at start and again after every `SettingsChanged`.
- **Not yet bindable:** punctuation keys, such as `Ctrl+,` or `Ctrl+=`. Their virtual keys move with the keyboard layout, so a stored name would not say which key was meant.

Rejected:

- **A fixed keymap**, as this spec recorded until 2026-11-01. It deferred bindings until there was a settings format to hold them, not because fixed bindings were wanted. The maintainer asked for user-editable bindings, and `Settings.shortcuts` is that format.
- **Storing the whole keymap.** Every default would be frozen in each user's settings, so a default changed in a later release would never reach them.
- **Recording in the interface.** A bound chord never reaches it, so the page could not record Ctrl+T or any chord already in use.
- **Refusing every Ctrl+Alt chord** for AltGr's sake. Most layouts type nothing with them, and they are the chords least likely to collide with pages.

## Focusing the address field

Focusing the address field from a page first moves keyboard focus to the chrome webview of the tab's window, then emits `AddressFocusRequested` to that chrome alone, with the tab it is for. The event names the tab because Ctrl+T requests focus for a tab the interface has not rendered yet. The interface focuses the field when that tab is active, and holds the request until it is. A popup has no address field, so there it does nothing, and focus stays on the page.

## Reopening closed tabs

Closing a tab remembers it, with its position, its whole back and forward history, its scroll and the form contents Rust holds for it ([Tab optimization](tab-optimization.md)). Ctrl+Shift+T reopens the most recent one under a new id, in the window it was closed in, or in the browser window used last if that one has closed. A web page comes back discarded, so it loads into whatever slot it gets and its scroll and drafts are put back as for any discarded tab; an internal page is drawn by the chrome again.

The position is the index the tab had when it closed, clamped to the strip as it is now. That is exact when nothing else changed in between; after other tabs were opened or closed, the tab lands near where it was rather than exactly there.

- The last **25** closed tabs are remembered (`CLOSED_LIMIT`). Each holds no webview, only Rust's record of the tab: its history, scroll, drafts and page reading. That is small, so the limit is about what is useful, not memory.
- They are kept **in memory only**, not in the session. Reopening across a restart would make the session format carry them; nothing asked for it yet.
- A tab that never left an internal page, such as a new tab opened and closed again, is not remembered. Reopening it would bring back nothing, and it would shadow the tab the user meant.
