---
status: DRAFT
last_updated: 2026-10-23
scope: What Haku does when a process behind a page, the interface or the whole engine fails, and what the user sees.
---

# Crash recovery

A page's renderer process can exit: it crashed, ran out of memory, or was ended from Task Manager. WebView2 reports it on the webview with `ProcessFailed`, and today nothing in Haku listens. The tab keeps claiming a live page, its slot holds a dead one, and the only way back is closing the tab.

## What fails

`ProcessFailed` names the kind of process and, on newer runtimes, a reason. Haku answers the kinds by what they take down:

| Kind                                                        | What is lost                                           | Haku's answer                        |
| ----------------------------------------------------------- | ------------------------------------------------------ | ------------------------------------ |
| `RENDER_PROCESS_EXITED` on a content webview                | That page                                              | [The tab is crashed](#a-crashed-tab) |
| `RENDER_PROCESS_EXITED` on a chrome webview                 | That window's interface                                | [Reload it](#the-interface)          |
| `BROWSER_PROCESS_EXITED`                                    | Every webview, the interface included                  | [Restart Haku](#the-engine)          |
| `FRAME_RENDER_PROCESS_EXITED`                               | One frame inside a page; the engine draws its sad face | Nothing                              |
| `GPU_PROCESS_EXITED`, utility, sandbox helper, plugin kinds | Nothing lasting; the engine starts them again          | Nothing                              |
| `RENDER_PROCESS_UNRESPONSIVE`                               | Nothing yet: the page is hung, not gone                | [Not built](#not-built)              |

The reason, such as `OUT_OF_MEMORY` or `TERMINATED`, does not change the answer. It is kept on the tab to word the notice.

Which slot failed is known from the webview that raised the event, as every other page signal is ([Page observation](page-observation.md)). Two pages of the same site may share a renderer process; each of their webviews reports its own failure, so each tab is crashed on its own. This is checked when it is implemented.

## A crashed tab

The tab becomes **crashed**, a new `TabPresence`:

- Its slot is released, as for a discard: hidden and navigated to `about:blank`, which gives the webview a fresh renderer for the next tab to use. The page is not read as it is left, since there is no page left to read.
- A dialog it was waiting on is released, and a connected page loses its connection, as on a discard.
- `realize` never binds a crashed tab, even the visible one. A page that crashes as it loads would otherwise crash, reload and crash again for good.
- Reload, by the toolbar, F5 or the notice's button, makes it discarded again, and it loads as a discarded tab does. `reload` already loads a tab that has no slot, so this needs no new command.
- Navigating it anywhere, going back or forward, closing it and reopening it work as for a discarded tab.
- The session stores it like any tab. On the next launch it is an ordinary discarded tab: a restart is the user starting again.

Crashed is a presence rather than a flag on a discarded tab because it answers a different question: the pool must not load it on its own, and the interface draws it differently. A flag would leave every place that binds or draws a discarded tab to remember a second condition.

### What the user sees

- **The active tab**: in place of the page, a notice in the style of the other failure screens ([`crash-style.ts`](../../src/app/crash-style.ts)): "This page crashed", one line naming why when the reason says (out of memory, or ended by another program), and a Reload button. The chrome is solid over the viewport while it shows, as for an internal page.
- **In the tab strip**: the tab's icon is replaced by a warning icon, so a tab that crashed in the background is found without opening every tab.
- Nothing else announces it: no toast. A background tab crashing is not urgent, and the strip shows it.

A crashed tab is not reloaded automatically, as Chrome and Edge do not. The rejected alternative, reloading once and showing the notice only if it crashes again, hides a crash that lost the user's form input and costs the memory the crash may have been about.

## The interface

When a chrome webview's renderer exits, the window shows nothing, and since Haku draws its own window frame, not even a way to close it. The window's interface is reloaded: the page is the same bundle, and the state it shows comes from Rust ([Architecture](../architecture.md)), so nothing is lost but what the interface held itself, such as an address being typed.

A reload is attempted at most three times a minute per window. Past that the interface is failing as it starts, and reloading again would only loop; the window is left as it is, and Alt+F4 still closes it.

## The engine

When the browser process exits, every webview of the profile is gone, the interfaces with them, and WebView2 cannot recreate them inside the same windows. The session is saved and Haku restarts itself, reopening every window and tab as discarded, as after any restart.

Restarting was chosen over rebuilding each window in place: rebuilding means recreating every window's chrome and every slot through paths that exist only for startup, to arrive where a restart arrives anyway.

- Tauri's restart starts the new process before the old one exits. The single-instance guard ([Windows and popups](new-windows.md#launching-again)) would take the new process for a second launch, hand it to the dying one and quit, leaving no Haku at all; the guard is released first, with the plugin's `destroy`.
- A browser process that exits as it starts would restart forever. The restart passes its time to the new process as an argument, and a failure within a minute of it shows a native message box saying the engine keeps failing, then quits instead.

## Testing

- The browser model's rules are unit tests: a crash releases the slot and dismisses the dialog; a crashed tab stays unbound when visible and when capacity frees; reload, navigation and closing behave as on a discarded tab; the session stores it as a plain tab.
- In the app, a renderer is made to crash with the DevTools protocol's `Page.crash` on a tab's webview, and its process can be ended from Task Manager. The interface's renderer is crashed the same way on the chrome webview, and the browser process by ending the `msedgewebview2.exe` started without `--type`.

## Not built

- A page that stops responding. WebView2 reports `RENDER_PROCESS_UNRESPONSIVE` when a page ignores input for some seconds, and keeps reporting it while the page stays hung. Chrome asks whether to wait or end the page. It needs a prompt that can be answered repeatedly and a way to end a renderer, and is a feature of its own.
- Telling the memory pressure logic that a page ran out of memory.
