---
status: IMPLEMENTED
last_updated: 2026-10-30
---

# Errors

This document covers how failures are represented and handled in Haku.

## Principle

Users are never shown a raw error. Every failure is classified where it happens, in Rust, and reaches the interface as a typed variant the interface decides how to present.

This scope covers only actual failures. Informational and success messages are out of scope.

## The shape

One enum, `HakuError`, defined in `tauri/src/error.rs` and exported to TypeScript by `pnpm bindings`:

| Variant           | Meaning                                                                                                                                      |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `TabNotFound`     | No tab has this id.                                                                                                                          |
| `NoSlotAvailable` | The webview pool is full and every resident is protected.                                                                                    |
| `InvalidUrl`      | A URL could not be parsed.                                                                                                                   |
| `Unsupported`     | The request cannot be honoured: the platform has no implementation, the caller is not permitted, or the browser rejects its input or target. |
| `WindowMissing`   | A window or webview handle could not be resolved.                                                                                            |
| `Storage`         | A settings, session or database operation failed.                                                                                            |
| `Tauri`           | A Tauri operation failed.                                                                                                                    |

The set is deliberately coarse. The interface decides what to show; a small, stable set keeps the generated TypeScript union usable and means adding a failure mode rarely changes the boundary.

## Why not a general error library

Error handling is load-bearing for a browser — navigation failures, webview crashes, storage faults — and the failure modes are Rust-originated. A purpose-built enum that mirrors exactly what Rust can return is a few dozen lines and cannot drift. `@codenhub/error` was considered and deliberately not used: it has breaking changes pending, and this is the wrong place to absorb churn.

## Rust

Every fallible function returns `error::Result<T>`. Conversions from `tauri::Error`, `rusqlite::Error` and `std::io::Error` are implemented, so `?` works throughout.

Document failure conditions with an `# Errors` section on any public function that returns `Result`.

## TypeScript

Generated commands resolve to `{ status: "ok", data } | { status: "error", error }`. `unwrap` in `src/ipc/result.ts` turns the error branch into a thrown `HakuFailure`, which is an ordinary `Error` — so `catch` blocks behave normally — while keeping the typed variant on `.cause` for anything that needs to branch on it.

```ts
const entries = await unwrap(commands.recentHistory(200));
```

Import `commands` from `@ipc/commands`, not from `@bindings`. The wrapper reports every failed command to one handler before handing the result back unchanged, and the interface points that handler at an error toast (`notifyFailure` in `features/feedback`). `void commands.foo()` is therefore safe: a failure is shown, not lost. The same failure repeated within a few seconds is shown once, because some commands run on a timer.

## Failures no command is waiting on

Some failures happen with no command call to reply to: a shortcut, a page's request for a new window, or startup. Rust reports these with one event, `FailureReported`, whose payload is a `HakuError`. The interface hands it to the same handler as a failed command, so it is shown the same way.

It is sent to one window's interface, never to all of them, so a failure is shown once:

| Failure                                                                                       | Sent to                                                                                           |
| --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| A shortcut, private windows' Ctrl+Shift+N included                                            | The window the shortcut was pressed in                                                            |
| A page's request for a new tab or window, a private window included                           | The window of the tab that asked, or the normal window used last for an extension's toolbar popup |
| Launching Haku again while it runs, which opens a window                                      | The normal window used last before it                                                             |
| The history file cannot be opened at startup (see [below](#failures-that-must-not-propagate)) | The first interface to load its state, once                                                       |

A failure at startup happens before any window exists, so Rust holds it until an interface can listen. The interface attaches its listeners before it asks for its first state, so `get_state` is the point at which a window is known to be listening: the first one to call it is sent every held failure, and they are not held after that. A later window, or a reloaded interface, is not sent them again.

Each window listens for its own webview only, as for `StateChanged`, because a listener for any target also receives what Rust sends every other window.

Alternatives rejected (decided 2026-10-30):

- **Sending to every window.** The same failure would appear in each, for one action in one of them.
- **Sending a startup failure as soon as a window exists, or after a delay.** The event would be lost whenever the interface had not yet attached its listener, which is exactly the case at startup.
- **A command the interface calls to collect held failures.** It would add a second contract to do what `get_state` already marks: that the interface is listening.
- **An event per kind of failure, such as one for history.** The interface would need a new listener and message for each, where `HakuError` already describes the failure.
- **A general notification event, with levels for information and success.** Out of scope: this document covers failures only.

## Failures that must not propagate

Some operations are best-effort by nature and must never break the flow around them:

- Recording a visit. Losing a history row is better than failing a navigation.
- Reading a JSON store. A corrupt settings file yields defaults rather than refusing to start.
- Saving the session. It is rewritten on the next change.
- Opening the history file at startup. A file another Haku holds locked, or one that cannot be read, would otherwise stop Haku before it has a window to say why, so history is kept in memory for that run. Clearing history then clears the file too, and reports the failure when it still cannot be opened, so cleared visits never come back unannounced. The user is told once, through [`FailureReported`](#failures-no-command-is-waiting-on), that visits will not be kept past this run.

These swallow their error deliberately, each with a comment saying why.
