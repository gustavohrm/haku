# Page observation

**Status:** IMPLEMENTED
**Last updated:** 2026-10-01
**Scope:** How Haku learns what a content webview is showing, how that becomes tab history, and how dialogs a
page opens are shown and answered.

## The constraint

Content webviews load arbitrary remote pages and have no IPC. Everything Haku knows about a page is therefore
observed from Rust, never reported by the page. On Windows that means the `ICoreWebView2` events Tauri does
not surface, reached through `Webview::with_webview`. All of it sits behind `platform::observe_page` and
`platform::answer_dialog`.

## Navigation

### What goes wrong without it

A page can change its URL without loading a document: a single-page app's `pushState` and `replaceState`. A
page-load hook never sees those, so a tab kept the URL it was opened with, and returning to a suspended tab
reloaded an older page. Separately, a URL seen from outside does not say whether it should add a history entry:
a followed link should, a redirect or a replaced route should not.

### How it works

1. Every content webview runs `inject::navigation_log_script()`. It records each URL change the page makes,
   with the kind the page's own Navigation API reports: `push`, `replace`, `traverse` or `reload`. That covers
   the load that created the document (`navigation.activation`) and every same-document change after it
   (`currententrychange`).
2. WebView2's `SourceChanged` and `DocumentTitleChanged` events are the trigger. On either one, Rust evaluates
   `inject::navigation_drain_expression()` in the page with `ExecuteScript`, which returns and empties the log
   together with the current document title.
3. The result reaches `Browser::report_page` as ordered `Commit`s plus the title. The title is applied after
   the commits, so a route pushed and then retitled names the new entry, not the one the page left.

Rust reads the page; the page never calls Rust. A page can see and tamper with its own log, but the most it can
corrupt is its own tab's history.

Where the script never ran — an error page, the PDF viewer — the log is missing, and the webview's URL is used
instead: a new document counts as a new entry, anything else as a replacement.

### How a commit becomes history

| Case                                        | Effect on the tab's history                              |
| ------------------------------------------- | -------------------------------------------------------- |
| First commit after Haku navigated the slot  | Rewrites the current entry (it may have been redirected) |
| `push` — a link, a pushed route, a fragment | Adds an entry, keeping the current title until retitled  |
| `replace`, `reload`                         | Rewrites the current entry                               |
| `traverse` to an adjacent entry             | Moves to that entry                                      |
| `traverse` anywhere else                    | Rewrites the current entry                               |
| `about:blank`                               | Ignored: it is a parked slot, not a page                 |

"Haku navigated the slot" is tracked per slot: `bind` marks a slot as awaiting when it emits `EnsureSlot`, and
the next commit clears it.

### What counts as a visit

Browsing history (the SQLite store) records visits, which is a narrower thing than commits. `report_page` says
whether a report was one:

- **A visit:** a `push` commit, or the arrival of a page Haku sent the tab to — opening a tab, the address bar,
  back and forward.
- **Not a visit:** a suspended tab, or a restored session, reloading the page it already showed; a redirect; a
  replaced route; a reload; a retitle.

Anything that is not a visit retitles the latest visit to its URL instead, so a visit carries the title the
page settled on. Without the distinction every tab switch in a one-webview pool recorded a fresh visit.

### The webview's own back and forward

A pooled webview's native history holds pages from every tab that used its slot, so following it could land
on another tab's page. Every native cross-document back or forward — a mouse button, Alt+Left, a page's
`history.back()` — is cancelled in `NavigationStarting` and replayed through the tab's history instead
(`Browser::traversal`): forward if it names the next entry, otherwise back.

A same-document traversal cannot be intercepted, because the webview raises no `NavigationStarting` for it. It
arrives as a `traverse` commit and is matched against the adjacent entries.

### Known limits

- A report from a page that was just evicted can arrive after its slot has been given to another tab, and be
  attributed to that tab. The window is the few milliseconds between the command and the navigation.
- Cancelled native traversals reload the page instead of using the back-forward cache.

## Page dialogs

`alert`, `confirm`, `prompt` and the "Leave site?" `beforeunload` confirmation are drawn by the interface with
`@codenhub/toaster`, not by WebView2.

1. WebView2's default dialogs are disabled per content webview. `ScriptDialogOpening` takes a deferral, which
   keeps the page paused, and stores the event objects on the UI thread keyed by a `DialogId`.
2. `Browser::open_dialog` attaches the `PageDialog` to the tab owning the slot, and it reaches the interface in
   `Tab::dialog`.
3. The interface shows the active tab's dialog, titled with the requesting site's host so a page cannot pass
   its message off as Haku's. A background tab's dialog waits until that tab is selected.
4. The answer goes back through `answer_dialog` to `Effect::AnswerDialog`, which completes the deferral on the
   UI thread.

A dialog is never stranded. When its page goes away — the tab is closed, navigated, reloaded, or loses its slot
— `Browser` answers it first: a "Leave site?" is accepted, since leaving is what was happening, and anything
else is dismissed. A "Leave site?" raised while Haku itself is navigating the slot is accepted without asking,
because by then the slot may belong to a different tab.

### Known limits

- Closing a tab never asks "Leave site?". The tab is closed first and its page is released afterwards.
- A dialog is modal to the whole interface, not just its tab: switching tabs means answering it first.
- The page area cannot be dimmed behind a dialog, because the page is a native view below the chrome. The
  backdrop is transparent everywhere instead, so the bars and the page look the same.
