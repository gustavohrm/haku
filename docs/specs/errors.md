# Errors

**Status:** IMPLEMENTED
**Last updated:** 2026-09-24

This document covers how failures are represented and handled in Haku.

## Principle

Users are never shown a raw error. Every failure is classified where it happens, in Rust, and reaches the
interface as a typed variant the interface decides how to present.

This scope covers only actual failures. Informational and success messages are out of scope.

## The shape

One enum, `HakuError`, defined in `tauri/src/error.rs` and exported to TypeScript by `pnpm bindings`:

| Variant           | Meaning                                                           |
| ----------------- | ----------------------------------------------------------------- |
| `TabNotFound`     | No tab has this id.                                               |
| `NoSlotAvailable` | The webview pool is full and every resident is protected.         |
| `InvalidUrl`      | A URL could not be parsed.                                        |
| `Unsupported`     | The platform has no implementation, or a caller is not permitted. |
| `WindowMissing`   | A window or webview handle could not be resolved.                 |
| `Storage`         | A settings, session or database operation failed.                 |
| `Tauri`           | A Tauri operation failed.                                         |

The set is deliberately coarse. The interface decides what to show; a small, stable set keeps the generated
TypeScript union usable and means adding a failure mode rarely changes the boundary.

## Why not a general error library

Error handling is load-bearing for a browser — navigation failures, webview crashes, storage faults — and the
failure modes are Rust-originated. A purpose-built enum that mirrors exactly what Rust can return is a few
dozen lines and cannot drift. `@codenhub/error` was considered and deliberately not used: it has breaking
changes pending, and this is the wrong place to absorb churn.

## Rust

Every fallible function returns `error::Result<T>`. Conversions from `tauri::Error`, `rusqlite::Error` and
`std::io::Error` are implemented, so `?` works throughout.

Document failure conditions with an `# Errors` section on any public function that returns `Result`.

## TypeScript

Generated commands resolve to `{ status: "ok", data } | { status: "error", error }`. `unwrap` in
`src/ipc/result.ts` turns the error branch into a thrown `HakuFailure`, which is an ordinary `Error` — so
`catch` blocks behave normally — while keeping the typed variant on `.cause` for anything that needs to branch
on it.

```ts
const entries = await unwrap(commands.recentHistory(200));
```

Import `commands` from `@ipc/commands`, not from `@bindings`. The wrapper reports every failed command to one
handler before handing the result back unchanged, and the interface points that handler at an error toast
(`notifyFailure` in `features/feedback`). `void commands.foo()` is therefore safe: a failure is shown, not
lost. The same failure repeated within a few seconds is shown once, because some commands run on a timer.

## Failures that must not propagate

Some operations are best-effort by nature and must never break the flow around them:

- Recording a visit. Losing a history row is better than failing a navigation.
- Reading a JSON store. A corrupt settings file yields defaults rather than refusing to start.
- Saving the session. It is rewritten on the next change.

These swallow their error deliberately, each with a comment saying why.
