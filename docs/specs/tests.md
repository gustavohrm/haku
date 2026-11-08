---
status: APPROVED
last_updated: 2026-11-08
scope: Test categories, locations, tooling, and coverage expectations for the interface and the Rust backend.
---

# Tests

This document defines how the project is tested: what kinds of tests exist, where they live, how they run, and what they are expected to cover. The conventions for writing an individual test are in the [coding guidelines](../guidelines/code.md#testing).

## Principles

- **Test behavior, not implementation.** A test verifies what a caller can observe, not how the code produces it.
- **Fast and deterministic.** A flaky test is a bug. Time, randomness, and the network come from sources a test controls; a test that depends on real timing, the network, or a real window does not belong in the default suite.
- **Error paths count.** Failure cases are tested as carefully as the happy path, and inputs and edge cases early.
- **No permanent absence checks.** A test asserting only that a removed name, class, or token is gone (`not.toContain("old-name")`) verifies nothing about current behavior: it is indistinguishable from asserting the absence of a string that never existed. Such a check earns its place only while a rename or removal is in flight, as a guard against a forgotten reference; delete it once the change is verified, in the same pull request. A durable rule, such as "no compatibility aliases", is a lint rule or repository check, not a permanent unit test.

## Categories

| Category    | Where                                                | Runs with             | Covers                                                                       |
| ----------- | ---------------------------------------------------- | --------------------- | ---------------------------------------------------------------------------- |
| Unit        | Next to the code: `src/<file>.test.ts` or `.tsx`     | `pnpm test`           | One module, its boundaries replaced by fakes.                                |
| Integration | `<package>/tests/integration/`                       | `pnpm test`           | Several modules together, or a real boundary such as a database or a server. |
| Browser     | `<package>/tests/e2e/` or `<package>/tests/browser/` | Its own script, below | Full user journeys, rendering, and visual regression in real browsers.       |

Unit and integration tests use [Vitest](https://vitest.dev/); browser tests use [Playwright](https://playwright.dev/). Add a category only when the project has tests for it: an empty suite, or a script that passes without running anything, misleads more than a missing one.

Manual usage scenarios, such as a playground used during development, are outside these categories.

### Unit tests

- A test file is named after the file it covers: `store.ts` is tested by `store.test.ts`.
- What to mock, and what to test against real temporary state instead, is owned by the [coding guidelines](../guidelines/code.md#testing).
- Do not import test utilities across distant packages, except from a package or subpath that exists to share them.

### Integration tests

- Integration tests live inside the package or app they belong to. Do not add a repository-root `tests/` directory by default.
- When a journey or orchestration genuinely spans packages, a dedicated workspace package for those tests is allowed, rather than placing them in one of the packages they cross.

### Browser tests

- Browser suites are configured and run apart from `pnpm test`, so the loop everyone runs constantly stays fast and needs no browser installed. `pnpm test`, `pnpm test:watch`, and `pnpm test:coverage` MUST NOT reach a browser suite, directly or through another script.
- A project with a browser suite adds a `test:browser` script, at the root and in each package that has one. [Tooling](../tooling.md) records whether `pnpm verify` runs it and how browsers are installed. A package does not install browsers from its own scripts.
- Every Playwright project name ends in the engine it runs on, `chromium`, `firefox`, or `webkit`, optionally prefixed to distinguish surfaces (`app-chromium`). CI can then run one engine per job by selecting on the name, and a project named anything else cannot silently drop out of that selection. [CI](../ci.md) describes how jobs select engines.
- A suite SHOULD cover every engine its users run, because catching what one engine does differently is what the suite is for.
- Add visual regression tests once a component has an agreed visual design, not for placeholder interfaces.
- A Playwright config MUST set `retries: process.env.CI ? 1 : 0`, unless it states why it needs more. A test that passes on its retry is reported as `flaky`, not as passed, so the retry keeps one timing failure from failing a merge without hiding it, and a test that fails twice still fails the run. Locally there is no retry, so a flake shows up where it can be fixed. The cost is that a flaky test stays green until someone reads the summary line; a test that shows up there is fixed rather than retried more. Without retries, codenhub/codenhub's full run failed 3 of 30 merges on 2026-11-03, each on one timing-sensitive Firefox test ([codenhub/codenhub@702af904](https://github.com/codenhub/codenhub/commit/702af904)).
- A suite whose tests do not read cookies, storage, or permissions left by an earlier test SHOULD share one browser context per worker and take a fresh page per test. Playwright builds a context per test by default, and the engines price that very differently: on one codenhub/codenhub suite a fresh context cost Firefox about 2.6 s against Chromium's 0.3 s, the whole gap between them. Writing such state does not disqualify a suite; reading it back across a test boundary does. A suite whose assertions rely on state an earlier test left behind MUST keep the default. A suite that shares a context MUST reset whatever the page under test persists before each test, not after, so the reset holds even when a test fails partway.

## Configuration

- A package or app that needs custom options, such as path aliases, environment variables, plugins, or a DOM environment, MUST include its own explicit configuration file, `vitest.config.ts` or `playwright.config.ts`, and not `vite.config.ts`: a runner that discovers packages by their `vitest.config.ts` would silently leave one configured through `vite.config.ts` out of `pnpm test`. Do not rely on implicit or undocumented fallback behavior. How the runs are put together is [tooling](../tooling.md)'s.
- Test scripts MUST NOT chain their own build step. Build ordering belongs to the root tooling, as [Tooling](../tooling.md) describes.
- A package that defines its own test scripts uses the root names and meanings:
  - `test`: runs unit and integration tests once, and nothing that needs a browser.
  - `test:watch`: runs those tests in watch mode.
  - `test:coverage`: runs those tests and writes a coverage report.
  - `test:browser`: runs the browser suite, when the package has one.
  - `test:browser:watch`: runs the browser suite in an interactive or UI mode, when the project offers one.

## Coverage

`pnpm test:coverage` writes a V8 coverage report to `coverage/`. The suggested target is **80%**. It guides development and review; it is not a gate and never fails a build, because a coverage threshold that blocks merges gets satisfied with tests that execute lines without asserting anything.

## Not covered by automated tests

Behavior that no automated test reaches is listed here with how it is verified instead, usually by hand, and why a suite was not added. A gap that is written down can be judged; a gap nobody recorded is found by a user.

<!-- project:start -->

Nothing drives the built app end to end. The behaviors that only a real webview shows — layering, the input mask, page observation — are verified by hand.

An end-to-end suite is not added for now (decided 2026-10-23). A WebDriver session drives a webview's document, and these behaviors live outside any document: in the order of native windows, the region of the chrome that takes input, and the engine's own events. A suite would test what unit tests already cover and miss what is checked by hand. A smoke test that the built app starts and its interface renders is the one worth adding, if startup regressions show up.

## What is tested where

The architecture is what makes most of Haku testable without a window. `browser::Browser` contains no Tauri types and returns `Effect`s rather than touching webviews, so the whole tab and pool policy is tested as plain Rust. A test that needs a real window is usually a sign that logic sits in the wrong module.

| Area                                                  | Tooling      | Location                                                         |
| ----------------------------------------------------- | ------------ | ---------------------------------------------------------------- |
| Browser policy, models, chrome geometry, storage, IPC | `cargo test` | In the module, or a sibling `*_tests.rs` included with `#[path]` |
| Interface logic, stores, hooks                        | Vitest       | Colocated `*.test.ts` / `*.test.tsx`                             |

Haku has no integration or browser suites.

### Rust

Unit tests live in a `#[cfg(test)] mod tests` in the module they cover. When a module's tests grow large enough to bury it, they move to a sibling file, as `browser_tests.rs` does for `browser.rs`, included with `#[path]` so they keep access to the module's private items.

The parts of `webview/` and `platform/` that call into real webviews and native windows are verified by running the app, not by unit tests. Their pure parts, such as the injected scripts in `webview/inject.rs` and `platform/workers.rs`, are unit tested like any other module, and logic that can move out of a window-bound function into a pure one should.

Rust coverage is not measured.

### Interface

Vitest runs under jsdom. Mock only real boundaries, such as the generated commands in `@bindings`. The 80% coverage target applies to the interface logic that can run under jsdom, in `src/`.

### Haku's commands

Haku is one package at the root, so a path after `pnpm test` names a file or directory under `src/`. Arguments after `--` reach `cargo test`, so a module path filters the Rust tests to that module.

```sh
pnpm test src/ipc/store.test.ts
pnpm test:rust
pnpm test:rust -- browser::tests
```

<!-- project:end -->

## Commands

Run tests from the repository root.

```sh
pnpm test
pnpm test:watch
pnpm test:coverage
```

Narrow a run while working on one area. A path after the script is forwarded to Vitest, so a single directory or file runs on its own:

```sh
pnpm test packages/<name>
pnpm test packages/<name>/src/<file>.test.ts
pnpm test:watch packages/<name>
```

[Tooling](../tooling.md) describes the other ways to narrow a run.
