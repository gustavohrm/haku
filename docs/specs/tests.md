---
status: APPROVED
last_updated: 2026-10-06
scope: Test categories, locations, tooling, and coverage expectations for the interface and the Rust backend.
---

# Tests

This document defines how Haku is tested: what kinds of tests exist, where they live, and what they are expected to cover. The conventions for writing an individual test are in the [code guidelines](../guidelines/code.md#testing).

## Philosophy

- **Test behavior, not implementation.** A test verifies what a caller can observe, not how the code produces it.
- **No permanent absence checks.** A test asserting only that a removed name is gone verifies nothing about current behavior. It earns its place only while a rename or removal is in flight; delete it once the change is verified, in the same pull request.
- **Fast and deterministic.** Flaky tests are unacceptable. A test that depends on timing, the network, or a real window does not belong in the default suite.
- **Error paths count.** Failure cases are tested alongside the happy path.

## What is tested where

The architecture is what makes most of Haku testable without a window. `browser::Browser` contains no Tauri types and returns `Effect`s rather than touching webviews, so the whole tab and pool policy is tested as plain Rust. A test that needs a real window is usually a sign that logic sits in the wrong module.

| Area                                                  | Tooling      | Location                                                         |
| ----------------------------------------------------- | ------------ | ---------------------------------------------------------------- |
| Browser policy, models, chrome geometry, storage, IPC | `cargo test` | In the module, or a sibling `*_tests.rs` included with `#[path]` |
| Interface logic, stores, hooks                        | Vitest       | Colocated `*.test.ts` / `*.test.tsx`                             |

### Rust

Unit tests live in a `#[cfg(test)] mod tests` in the module they cover. When a module's tests grow large enough to bury it, they move to a sibling file, as `browser_tests.rs` does for `browser.rs`, included with `#[path]` so they keep access to the module's private items.

The parts of `webview/` and `platform/` that call into real webviews and native windows are verified by running the app, not by unit tests. Their pure parts, such as the injected scripts in `webview/inject.rs` and `platform/workers.rs`, are unit tested like any other module, and logic that can move out of a window-bound function into a pure one should.

### Interface

Vitest runs under jsdom. Test files are colocated with the code they cover and named after it: `store.ts` is tested by `store.test.ts`. Mock only real boundaries, such as the generated commands in `@bindings`. Pure logic is tested directly.

## Coverage

`pnpm test:coverage` reports V8 coverage for `src/`. The suggested target is 80% of the interface logic that can run under jsdom. It is a target that guides review, not a gate: it never fails a run, because a coverage threshold that blocks merges gets satisfied with tests that execute lines without asserting anything.

Rust coverage is not measured.

## Not covered yet

Nothing drives the built app end to end. The behaviors that only a real webview shows — layering, the input mask, page observation — are verified by hand. An end-to-end suite is a decision of its own, with its own tooling, and belongs to this document when it is made.

## Commands

```sh
pnpm test
pnpm test src/ipc/store.test.ts
pnpm test:watch
pnpm test:coverage
pnpm test:rust
pnpm test:rust -- browser::tests
```

A file path after `pnpm test` runs that file alone. Arguments after `--` reach `cargo test`, so a module path filters the Rust tests to that module.
