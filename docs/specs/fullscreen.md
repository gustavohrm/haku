---
status: IMPLEMENTED
last_updated: 2026-10-31
scope: What happens when a page asks for fullscreen, such as a video player's fullscreen button, and how it ends.
---

# Fullscreen

Approved and implemented on 2026-10-31. It adds a field to `BrowserState`, so it is a contract decision under [Agent instructions § Judgment](../../AGENTS.md#judgment). [First release](../first-release.md#features) first listed the feature as filling only the window; this spec widened it to the whole screen (see [Decisions](#decisions)).

## Problem

A page enters fullscreen with the Fullscreen API, as YouTube's fullscreen button does. Haku does not handle it. WebView2 makes the element fill the webview's own bounds, and those bounds are the viewport, so the video grows to the page area while the tab strip, the toolbar, the page margin and the taskbar stay on screen. Leaving fullscreen works, but only because nothing changed outside the page.

## Behaviour

**Entering.** When a page's document gains a fullscreen element, its window:

1. becomes an OS fullscreen window on the monitor it is on, covering the taskbar;
2. hides every bar, floating or docked, and stops revealing floating bars on hover;
3. shows that tab's page edge to edge: no margin, no rounded corners, no split divider. In a split pair the other pane is not shown;
4. shows a toast naming the site: "_youtube.com_ is full screen. Press Esc to exit." It closes on its own after a few seconds.

The engine only grants fullscreen in answer to a click or key, so a page cannot enter it on its own.

**Leaving.** The window returns to the size, position and maximized state it had before, with its bars and layout as they were. Fullscreen ends when:

| Cause                                                                             | How                                                        |
| --------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| The page exits, such as the player's button or a navigation                       | The engine reports it                                      |
| The user presses Esc                                                              | See [Esc](#esc)                                            |
| Another tab is selected, including the other pane of a split                      | The window leaves at once, then Haku asks the page to exit |
| The address field is focused, by Ctrl+L, Alt+D or F6                              | The same: the field is in the hidden bars                  |
| The page goes away: the tab closed, its process crashed, or its slot was given up | The window leaves; there is no page left to ask            |

Shortcuts keep working while fullscreen, so Ctrl+Tab or Ctrl+W leave it by the rule above. Focusing the address field leaves it too: the field is in the bars fullscreen hides, so focus would move to a field nobody can see, and Esc, which the engine reads only while the page has focus, would no longer end it (decided by the maintainer on 2026-11-01). Haku's own dialogs, page menus and toasts draw over a fullscreen page as they do over any other, and do not end it. Losing focus, as Alt+Tab does, does not end it either.

Popup windows and private windows behave the same. Each window is fullscreen on its own.

## Esc

The engine ends a page's fullscreen on Esc by itself and reports it like any other exit, so Haku binds no key: checked in the running app on 2026-10-31 (WebView2 156). Esc is otherwise passed to the page as today.

## Mechanism

**The signal.** `platform/` listens to WebView2's `ContainsFullScreenElementChanged` on every content webview and reports it as a page signal, like the others in [Page observation](page-observation.md). The page reports nothing itself.

**The state.** `Browser` keeps, per window, the tab whose page is fullscreen, if any. `realize()` clears it whenever that tab is no longer shown in a live slot of that window, which covers every "the window leaves" cause above in one place instead of in each operation. Selecting another tab clears it too: the other pane of a split pair stays assigned while the fullscreen pane covers it, so `realize()` alone would keep fullscreen while focus moved to a page that is not on screen. Fullscreen is not saved with the session.

**The contract.** `BrowserState` gains one field:

```ts
fullscreen: TabId | null; // the shown tab whose page is fullscreen
```

No command is added: entering and leaving start from the page or from a native key, never from the interface.

**The window.** Applying a change of that field sets the window's fullscreen through Tauri (`Window::set_fullscreen`), which saves and restores the window's placement.

**Asking a page to exit.** WebView2 has no call to end a page's fullscreen, so Haku runs `document.exitFullscreen()` in the page, as it already runs expressions there to read state ([Security § Injected scripts](../guidelines/security.md#injected-scripts)). The window does not wait for the page: it has already left.

**The interface.** The shell reads `fullscreen` and drops its bars and margins. The fullscreen pane fills the window; another pane on screen is reported with zero size, which [Chrome layering](chrome-layering.md#the-contract-with-the-interface) already defines as hidden while keeping its assignment. The pane set does not change, so `layoutRevision` does not advance.

## Decisions

- **The whole screen, not the window.** Chosen by the maintainer on 2026-10-31. Every major browser does this, and a video in a window-sized frame with the taskbar beneath it is what this spec exists to fix. Filling only Haku's window, as First release worded it, was rejected for that reason; First release is updated in the same change.
- **A toast naming the site.** A fullscreen page owns every pixel, so it could draw something that looks like Haku or Windows. Naming the site on entry is how browsers answer that. It goes through `@codenhub/toaster`, like every notification.
- **Rust owns the state, `realize()` clears it.** The interface could hide its bars from a page signal alone, but the window's OS fullscreen and the many ways a tab stops being shown belong to `Browser`, and a flag kept in each window's interface would have to repeat every one of them.
- **The tab, not the window, leaves first.** When another tab is selected the window leaves immediately and the page is asked afterwards, so a page that ignores the request cannot keep the window fullscreen.

## Not built

- **F11**, fullscreen for the whole browser with no page asking. A separate feature with its own questions (bars on hover, how to leave); this spec leaves F11 to the page and the engine.
- Remembering a page's fullscreen across a reload or a restart.

## Known limits

- A page can replace `document.exitFullscreen` with its own function. The window still leaves when another tab is selected, but that page keeps its fullscreen element and shows it inside the viewport when selected again, until it exits on its own or reloads.
