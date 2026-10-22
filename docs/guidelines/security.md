---
status: APPROVED
last_updated: 2026-10-21
scope: How the boundary between web pages and the application is kept in code, for anyone writing or reviewing a change.
---

# Security guidelines

[Architecture § Security](../architecture.md#security) says what the boundary guarantees, and [Page observation](../specs/page-observation.md) says how Haku learns about a page without the page taking part. This document says how code keeps those guarantees, so the same problem is solved the same way everywhere. Where it and the architecture document disagree, the architecture document wins and this one is fixed. Vulnerabilities are reported as [SECURITY.md](../../SECURITY.md) describes.

Content webviews load arbitrary remote pages, and they are given nothing. The rules below keep a page out of the application, and keep the chrome's own surfaces distinct from anything a page can draw.

## Capabilities

- Core permissions go to chrome webviews only. [`tauri/capabilities/default.json`](../../tauri/capabilities/default.json) scopes them to `webviews: ["window-*"]`. Content webviews are labelled `content-N` and extension popups `extension-popup-N`, and the pattern matches neither.
- Do not scope a capability to `windows`. A window scope also covers every child webview in it, which includes the content webviews.
- Do not add a remote origin to a capability. Tauri withholds IPC from remote origins unless a capability opts them in, and Haku opts none in.
- A new capability or permission is a contract change. Name it in the pull request and test it.

## Commands

- Every command takes the calling `tauri::Webview` and calls `ensure_chrome` before it does anything else. That rejects any caller that is not a window's chrome, and identifies the window the caller belongs to. Every command in [`tauri/src/ipc/commands.rs`](../../tauri/src/ipc/commands.rs) does this today.
- A command that acts on a window acts on the one that called it, never on another. Commands that change what every window shares, such as `set_settings` reconfiguring the pool, act on the whole browser by design, and say so in their documentation.
- A new command or event changes the IPC contract. It is proposed and approved before it is built, as [AGENTS.md](../../AGENTS.md) requires for contract changes.

## Page metadata comes from Rust

- Rust reads what a page is showing, the URL, title, icons, navigation, dialogs and memory state, through WebView2's events and `ExecuteScript`. Do not give a page a way to report these: no message handler, no host object, no request to a Haku endpoint. The page has no way to call back, and that is what keeps its claims from becoming commands.
- What Rust reads back is the page's own claim. Parse it into a typed value and drop what does not parse, as `parse_navigation_log` and `parse_page_state` do.
- A page's claim is applied to its own tab only: that tab's history, the text restored into it, and what discarding it would lose. The last reaches other tabs indirectly, because [Tab optimization § The rule](../specs/tab-optimization.md#the-rule) shares one budget: a page that claims unsaved work keeps its webview, which can leave less room for others. That is the price of never discarding work. Code that applies a claim must not reach past that tab in any other way.

## Injected scripts

- An injected script runs in the page's own world, so the page can read and change what it records. What it keeps is a record for Rust, in `window` globals such as `__hakuState` and `__hakuNavigation`. Treat it as the page's claim, as above.
- Record only what a reload would lose, and never a secret. The draft leaves out passwords, payment details, one-time codes, hidden and file fields, and anything marked `autocomplete="off"`, as [`inject.rs`](../../tauri/src/webview/inject.rs) documents.
- Restore a draft or scroll position only on the origin it was read on, so a reload that redirects to another site never receives text typed on this one.
- Store nothing in the page or on the site. Scroll and drafts are held by Rust, [Tab optimization § Scroll and drafts live in Rust](../specs/tab-optimization.md#scroll-and-drafts-live-in-rust) explains why.
- The extension adjustments run only in `chrome-extension://` pages, never in an extension's background worker. See [Extensions § Adjustments injected into extension pages](../specs/extensions.md#adjustments-injected-into-extension-pages).

## Dialogs and what the chrome draws

- The chrome draws every dialog a page opens, titled with the requesting site's host, so a page cannot pass its text off as Haku's. [Page observation § Page dialogs](../specs/page-observation.md#page-dialogs) describes the flow.
- Anything that removes a page first answers its pending dialog: a tab closed, navigated, reloaded, or losing its slot. A dialog is never stranded.
- A page is a native view below the chrome, so it cannot paint over the chrome, and what the chrome draws over a page is opaque. [Chrome layering](../specs/chrome-layering.md) describes the mechanism, and [First release § What the chrome can draw over a page](../first-release.md#what-the-chrome-can-draw-over-a-page) lists the limits.

## Haku's own pages

- `haku://` pages are drawn by the chrome, not loaded into a webview, and they talk to Rust directly. A request from a page never opens one, whatever its target. See [Windows and popups](../specs/new-windows.md#the-request).

## Extension pages

- An extension page is loaded like any other page and is given no IPC. Its extension APIs are adjusted by one injected script, and an extension's background worker sees the engine's windows unadjusted ([Extensions § Limitations](../specs/extensions.md#limitations)).
- A toolbar popup is a window labelled `extension-popup-N`. `ensure_chrome` rejects it, so it cannot call a command, and the page it shows is no tab.

## Persisted files

- Settings, the open-tab session and history are files a user may edit by hand, JSON for the first two and SQLite for history ([Architecture § storage](../architecture.md#storage--what-survives-a-restart)). Read them as untrusted input.
- `read_json` layers a stored document over the defaults. A file that does not parse is set aside and replaced by the defaults. Nothing else in a file is trusted: `Settings::sanitized` makes a hand-edited value survivable, such as a pool capacity of zero, and a stored home page must have a scheme Haku opens.
- A draft is never written to the session file. Tabs skip it when serialised.

## Secrets and logging

- Haku stores no credentials, and it logs nothing today: `tauri/Cargo.toml` has no logging dependency. A change that adds logging keeps page content, form values, drafts and dialog text out of it.
- History and the session hold visited URLs, which can carry tokens in their query strings. Logs take the same care, and record a path rather than a full URL.

## Reviewing a change

A change that touches any of the above names it in its pull request and adds or updates its tests. The [pull request template](../../.github/pull_request_template.md) carries the check. Ask:

- Does it add or change a capability, permission, command or event? Is it named and tested?
- Does each new command call `ensure_chrome` before anything else, and act only on its window unless it changes shared state by design?
- Does a page now reach Rust, a chrome surface or a `haku://` page by a new path? That changes the boundary, so decide it first.
- Does a new injected script record a secret, or anything a reload would not lose?
- Is a value read back from a page parsed, and applied to its own tab only?
- Is every dialog or page-supplied text shown with its site's host?
- Is everything read from disk checked on read?
- Does a log line or an error carry page content, form values or a full URL?
