---
status: APPROVED
last_updated: 2026-11-12
scope: How the boundary between web pages and the application is kept in code, for anyone writing or reviewing a change.
---

# Security guidelines

This document says how code handles secrets, untrusted input, and what leaves the process, so the same problem is solved the same way everywhere. Vulnerabilities are reported as [SECURITY.md](../../SECURITY.md) describes, never in a public issue.

Content webviews load arbitrary remote pages, and they are given nothing. The rules below keep a page out of the application, and keep the chrome's own surfaces distinct from anything a page can draw.

[Architecture § Security](../architecture.md#security) says what the boundary guarantees, and [Page observation](../specs/page-observation.md) says how Haku learns about a page without the page taking part. Where this document and the architecture document disagree, the architecture document wins and this one is fixed.

## Secrets

When the project stores or issues secrets:

- **Random values** for anything security-relevant come from a cryptographically secure source, such as `crypto.getRandomValues` or `crypto.randomUUID`, injected where code needs to be testable. Never `Math.random`.
- **High-entropy tokens** the project issues, such as session ids, one-time codes, reset tokens, and API keys, are stored as a digest, never as the value. A leaked database then holds nothing a client can present.
- **Passwords** go through a memory-hard key derivation function where the runtime allows, with the algorithm and parameters stored next to each hash so they can be upgraded on the next login.
- **Recoverable secrets**, ones the project must read back, are encrypted with an authenticated cipher, with a key id stored beside each ciphertext so the key can be rotated.
- **Comparisons** of a secret or digest against input are constant-time.
- **Showing a secret** happens once, in the response to the action that created it, and that response is never cached.
- **Secrets in the repository** do not exist. Configuration reads them from the environment or a secret store; examples use obvious placeholders.

## Never in logs, errors, or responses

- An error message names the field, never its value.
- An unexpected error reaches the caller as a generic failure. Its detail goes only to the log, which redacts known secret-bearing fields.
- URLs can carry codes and tokens in their query strings. Log the path, not the full URL.
- Page content, form values, and anything a user typed stay out of logs.
- Where a leak would matter, a test proves it: it feeds known secrets through the flow and asserts they never appear in what was logged, returned, or stored.

## Untrusted input

- Every input crossing a trust boundary is validated at that boundary, before any other code sees it, and parsed into a typed value. What does not parse is rejected, not passed along.
- Every input has a size bound, such as a request body, an uploaded file, or a file read from disk, and every string and collection in a schema has a maximum. The read itself enforces the bound: the read consumes at most one byte past the limit, and when that byte exists the input is rejected, never truncated and parsed as if it were whole. Checking the size after reading the whole input is too late, because the memory is already used. A size check before the read can reject an input early, but it is not the bound, because another program can grow a file between the check and the read.
- Files a user can edit by hand, such as settings, sessions, and caches, are untrusted input too. A file that does not parse is set aside and replaced with defaults, and every value read from one is checked as if it came from a request.
- Do not build URLs, paths, or redirects from values a client controls, such as the `Host` or `X-Forwarded-*` headers. A redirect target comes from an allowlist and is compared exactly.
- Do not build a query, command, or HTML by concatenating input. Use parameters, argument arrays, and escaping by default.

## Web responses and sessions

When the project serves web requests:

- Every response goes through one place that sets the security headers, including a Content Security Policy. A route never loosens them.
- No inline script, unless it carries a per-response nonce.
- Authenticated pages and every response holding a secret are not cached.
- Session cookies are `HttpOnly`, `Secure`, and `SameSite=Lax` or stricter, and `__Host-` prefixed where possible.
- Every state-changing request uses a non-GET method and is protected against cross-site request forgery.
- Every authorization check runs on the server, declared where the route is defined, before the handler runs.

## Outbound calls and dependencies

- The project calls only services it needs for the feature at hand, or that its operator configured. No telemetry, update checks, or third-party lookups without a recorded decision and, where users are affected, a way to turn them off.
- Every dependency is a trust decision. The rule in [Coding guidelines](code.md#architecture-and-design) on adding dependencies applies with extra force to anything that runs at install time or handles secrets.

## Capabilities

- Core permissions go to chrome webviews only. [`tauri/capabilities/default.json`](../../tauri/capabilities/default.json) scopes them to `webviews: ["window-*"]`. Content webviews are labelled `content-N` and extension popups `extension-popup-N`, and the pattern matches neither.
- Do not scope a capability to `windows`. A window scope also covers every child webview in it, which includes the content webviews.
- Do not add a remote origin to a capability. Tauri withholds IPC from remote origins unless a capability opts them in, and Haku opts none in.
- A new capability or permission is a contract change. Name it in the pull request and test it.

## Commands

- Every command takes the calling `tauri::Webview` and calls `ensure_chrome` before it does anything else. That rejects any caller that is not a window's chrome, and identifies the window the caller belongs to. Every command in [`tauri/src/ipc/commands.rs`](../../tauri/src/ipc/commands.rs) does this today, and `ipc::tests::every_command_guards_with_ensure_chrome` fails when a registered command does not.
- A command that acts on a window acts on the one that called it, never on another. Commands that change what every window shares, such as `set_settings` reconfiguring the pool, act on the whole browser by design, and say so in their documentation.
- A new command or event changes the IPC contract. It is proposed and approved before it is built, as [AGENTS.md](../../AGENTS.md) requires for contract changes.

## Page metadata comes from Rust

- Rust reads what a page is showing, the URL, title, icons, navigation, dialogs and memory state, through WebView2's events and `ExecuteScript`. Do not give a page a way to report these: no message handler, no host object, no request to a Haku endpoint. The page has no way to call back, and that is what keeps its claims from becoming commands.
- What Rust reads back is the page's own claim. Parse it into a typed value and drop what does not parse, as `parse_navigation_log` and `parse_page_state` do.
- A page's claim is applied to its own tab only: that tab's history, the text restored into it, and what discarding it would lose. The last reaches other tabs indirectly, because [Tab optimization § The rule](../specs/tab-optimization.md#the-rule) shares one budget: a page that claims unsaved work keeps its webview, which can leave less room for others. That is the price of never discarding work. Code that applies a claim must not reach past that tab in any other way. One permanent exception: a shown page's fullscreen report takes its whole window fullscreen, and ends the fullscreen of the other pane in that window ([Fullscreen](../specs/fullscreen.md)). It stays within one window, keeps one fullscreen tab per window, and reaches the other page only with the fixed `document.exitFullscreen()` expression, carrying nothing the page supplied.

## Injected scripts

- An injected script runs in the page's own world, so the page can read and change what it records. What it keeps is a record for Rust, in `window` globals such as `__hakuState` and `__hakuNavigation`. Treat it as the page's claim, as above.
- Record only what a reload would lose, and never a secret. The draft leaves out passwords, payment details, one-time codes, hidden and file fields, and anything marked `autocomplete="off"`, as [`inject.rs`](../../tauri/src/webview/inject.rs) documents.
- Restore a draft or scroll position only on the origin it was read on, so a reload that redirects to another site never receives text typed on this one.
- Store nothing in the page or on the site. Scroll and drafts are held by Rust, [Tab optimization § Scroll and drafts live in Rust](../specs/tab-optimization.md#scroll-and-drafts-live-in-rust) explains why.
- The extension adjustments run only in `chrome-extension://` pages, never in an extension's background worker. See [Extensions § Adjustments injected into extension pages](../specs/extensions.md#adjustments-injected-into-extension-pages).

## Dialogs and what the chrome draws

- The chrome draws every dialog a page opens, titled with the requesting site's host, so a page cannot pass its text off as Haku's. [Page observation § Page dialogs](../specs/page-observation.md#page-dialogs) describes the flow.
- Anything that removes a page first answers its pending dialog: a tab closed, navigated, reloaded, or losing its slot. A dialog is never stranded.
- A page is a native view below the chrome, so it cannot paint over the chrome, and it never reads the chrome's pixels: what the chrome draws over a page may let the page show through, but never the other way. [Chrome layering](../specs/chrome-layering.md) describes the mechanism, and [First release § What the chrome can draw over a page](../first-release.md#what-the-chrome-can-draw-over-a-page) lists the limits.

## Haku's own pages

- `haku://` pages are drawn by the chrome, not loaded into a webview, and they talk to Rust directly. A request from a page never opens one, whatever its target. See [Windows and popups](../specs/new-windows.md#the-request).

## Extension pages

- An extension page is loaded like any other page and is given no IPC. Its extension APIs are adjusted by one injected script, and an extension's background worker sees the engine's windows unadjusted ([Extensions § Limitations](../specs/extensions.md#limitations)).
- A toolbar popup is a window labelled `extension-popup-N`. `ensure_chrome` rejects it, so it cannot call a command, and the page it shows is no tab.

## Persisted files

- Settings, the open-tab session and history are files a user may edit by hand, JSON for the first two and SQLite for history ([Architecture § storage](../architecture.md#storage--what-survives-a-restart)). Read them as untrusted input.
- `read_json` layers a stored document over the defaults. A file that does not parse, or is larger than 16 MB, is set aside and replaced by the defaults. Every file Haku reads whole, JSON documents, extension files, and picked bookmark and extension files, goes through `storage::read_bounded`, which stops one byte past that file's limit; SQLite reads its own databases a page at a time (decided 2026-11-12). Nothing else in a file is trusted: `Settings::sanitized` makes a hand-edited value survivable, such as a pool capacity of zero, a stored home page must have a scheme Haku opens, and a search URL must be a web address. A restored tab whose address has any other scheme is left out of the session.
- A draft is never written to the session file. Tabs skip it when serialised.

## Logging

- Haku stores no credentials, and it logs nothing today: `tauri/Cargo.toml` has no logging dependency. A change that adds logging keeps page content, form values, drafts and dialog text out of it.
- History and the session hold visited URLs, which can carry tokens in their query strings.

## Reviewing a change

A change that touches any of the above names it in its pull request and adds or updates its tests. The [pull request template](../../.github/pull_request_template.md) carries the check. Ask:

- Does it add a way for untrusted input to reach the code? Is that input validated and bounded at the boundary?
- Does a secret, token, user's data, or full URL now reach a log, error, or response, other than the one that shows a new secret to its creator?
- Does it store a secret, and is it stored as a digest or encrypted as above?
- Does it add a permission, capability, endpoint, or outbound call? Is that named in the pull request and tested?
- Does it read a file a user can edit, and check what it reads?

- Does it add or change an IPC command or event? Is that named in the pull request, and is the change to an existing one approved as a contract change?
- Does each new command call `ensure_chrome` before anything else, and act only on its window unless it changes shared state by design?
- Does a page now reach Rust, a chrome surface or a `haku://` page by a new path? That changes the boundary, so decide it first.
- Does a new injected script record a secret, or anything a reload would not lose?
- Is a value read back from a page parsed, and applied to its own tab only?
- Is every dialog or page-supplied text shown with its site's host?
