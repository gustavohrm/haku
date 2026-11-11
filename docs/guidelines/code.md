---
status: APPROVED
last_updated: 2026-11-09
scope: Coding conventions for every package and app in the workspace.
---

# Coding guidelines

This document outlines the patterns, conventions, and guidelines to follow when working on this codebase. The [testing specification](../specs/tests.md) owns how tests are organized and run, and the [security guidelines](security.md) own how code keeps the project's security commitments.

## Naming

- Files and folders: kebab-case (`user-service.ts`, `api-routes/`).
- Variables and functions: camelCase (`getUserById`, `isValid`).
- Classes and types: PascalCase (`UserService`, `ApiResponse`).
- Constants: UPPER_SNAKE_CASE (`MAX_RETRY_ATTEMPTS`).
- Booleans: prefixed with is, has, can, or should (`isLoading`, `hasPermission`).
- Arrays: plural nouns (`users`, `items`).
- Functions: verb phrases (`calculateTotal`, `fetchUser`).

Name by domain intent. Avoid names that say only the type or the mechanism, such as `data`, `item`, `manager`, or `helper`, unless the scope makes the meaning obvious.

## Architecture and design

- Single responsibility: each function, class, and module does one thing well.
- High cohesion: logic that belongs together stays together.
- Loose coupling: minimize dependencies between modules.
- Prefer dependency injection to hard-coding. Whatever makes code hard to test or tied to one environment, such as the clock, randomness, the network, and storage, arrives as a dependency.
- Group related files in a folder with an `index.ts` entrypoint.
- Keep each package's public surface small and intentional.
- Do not add compatibility layers unless there is a real consumer, persisted data, shipped behavior, or explicit requirement that needs one.
- Do not add a dependency unless simple in-house code is clearly worse.

[Architecture](../architecture.md) records Haku's layout, its layers and their dependency direction, the threading model, and the IPC boundary between the interface and Rust. [Architecture § Dependencies](../architecture.md#dependencies) applies the dependency rule above to runtime dependencies and says what it covers. [Errors](../specs/errors.md) defines how a failure is represented and shown.

### Fix it upstream

When a dependency lacks something or gets it wrong, the fix goes upstream where the project can report it, and the project takes it from a release. A local workaround is temporary and names the issue it waits on, as [a defect found mid-task](../../CONTRIBUTING.md#a-defect-found-mid-task) describes. That holds for a third-party dependency as much as for one the project or its owner maintains; only the latter is fixed upstream by the project itself.

## Code

- Functions are small and clearly nameable.
- Prefer `const` to `let`; never use `var`.
- Avoid magic numbers and strings: extract a value to a named constant when it has domain meaning or is reused.
- Code should explain itself through its names.
- Design for predictable, deterministic behavior. Time and randomness come from an injected source, so a test controls them.
- Avoid god objects, feature envy, and long method chains.
- Prefer readable code to clever code.

Literal values MAY stay inline when extraction makes code worse, such as protocol literals, header names, package names, CSS selectors, event names, SQL, test fixtures, one-off validation messages, and external API values used once near their boundary.

Function parameters:

- 0–1 parameters: excellent.
- 2 parameters: acceptable.
- 3 or more: refactor into a typed options object.

Exceptions: framework callbacks, test helpers, tiny local functions, and APIs where an object parameter would hide meaning instead of improving it.

## TypeScript

- Use strict TypeScript; `tsconfig.base.json` sets the flags.
- Prefer `unknown` to `any`. Validate `unknown` at the boundary before use.
- Prefer interfaces for object shapes and type aliases for unions and intersections.
- Use type-only imports when importing only types.
- Prefer string-literal unions or `as const` objects to `enum`. `enum` is not erasable syntax: Node.js type stripping rejects it, so code that `tsc` and Vitest accept can fail when Node.js loads it. Tests of `enum` support are the exception. An earlier rule allowed `enum` and named its members in UPPER_SNAKE_CASE; it was replaced once code run by Node.js directly hit the rejection ([codenhub/codenhub@e3ae18df](https://github.com/codenhub/codenhub/commit/e3ae18df)).
- Keep casts narrow and close to the boundary that justifies them.
- Do not export types or values that are not part of the package or module API.

## Modules and imports

- Prefer named exports, except where a tool or framework requires a default export, such as config files or framework routes.
- Use barrel files only as package or folder entrypoints.
- Do not deep-import another package's internals unless the subpath is part of its `exports`.
- Keep dependency direction clear: shared code does not import app-specific code.

## Async and errors

- Use `async`/`await` for asynchronous code.
- Run independent async work in parallel with `Promise.all()`. Sequential awaits in a loop need a reason, given in the comment on the lint suppression they require.
- Do not swallow errors silently.
- Throw or return errors with enough context to debug them.
- Do not log secrets, tokens, credentials, or private user data.
- `console` is for examples, CLIs, debugging utilities, entrypoints reporting startup failures, and deliberate diagnostics. Casual logging is not a substitute for error handling.

## Inputs, failures, and state

Most review findings that hold up are about these, not about style: a [study of the review threads in codenhub/codenhub](https://github.com/codenhub/codenhub/blob/37b44e7adc31f628c94fb9ff8bd257c869f64996/packages/skills/docs/internal/coding-skill-research.md), a monorepo whose code agents wrote, records the evidence.

- Validate input from outside the code's control after it is converted, against the limits of what consumes it: `--timeout 1e308` passes "positive number" and becomes an infinite timer. Reject empty, whitespace-only, NaN, Infinity, and partial numbers such as `10s`.
- When required input is missing or matches nothing, fail with a message naming it. A run whose required input was absent never reports success.
- Update in-memory state only after a write succeeds, and never swallow a write error.
- Apply a new option or fix to every path the same input or behavior flows through: each source of the value, sync and async variants, create and update, and every caller. Name any path left out, with the reason.
- A hand-written parser rejects syntax it does not support, naming the file and line, instead of reading it as data.
- Keep input order and any order a contract defines; sort output whose order is unspecified by a defined key with a tie-breaker.

## Testing

- Test behavior, edge cases, and error paths, not just the happy path.
- One behavior per test, named by the behavior it proves, as a sentence (`returns an empty list when the input is null`).
- Unit tests sit next to the code they test.
- Mock the network, timers, and browser APIs. Test code whose job is file or database access against a real temporary directory or database where practical, and mock them elsewhere. Pure logic is tested directly.
- Give each behavior decision a test that fails when the decision is reversed, and see a new test fail before keeping it.
- A test that writes files uses its own directory under the system temporary directory, never the package or the current directory, and removes it afterwards.
- Poll for a condition instead of sleeping a fixed time.
- Prefer observable behavior to implementation details.
- A change in behavior changes its tests in the same change.

Mocking the filesystem and storage everywhere was the earlier rule. It was narrowed because a mock answers only what the test tells it, so it cannot show the containment, symlink, and stale-output bugs found in review threads on [codenhub/codenhub#24](https://github.com/codenhub/codenhub/pull/24#discussion_r3875960231), [#25](https://github.com/codenhub/codenhub/pull/25#discussion_r3888570403), and [#293](https://github.com/codenhub/codenhub/pull/293#discussion_r4304630356); a real temporary directory does.

The [testing specification](../specs/tests.md) defines the test categories, configuration, scripts, and coverage target.

## Documentation in code

- Comments explain why, not what or how.
- Remove redundant comments, decorations, and outdated docs, in the change that makes them outdated.

Every symbol exported from a published package's entrypoint, through `package.json` `exports`, is that package's API and MUST have TSDoc in source, even when a README also documents it. A private package's exports SHOULD have it too: their callers are the project's own code, which a reviewer can read, so a missing tag costs less there. That includes functions, classes and their public members, interfaces, type aliases, constants, and configuration objects. The TSDoc describes the caller-facing purpose, important inputs, return values or side effects, and observable failure. It MUST NOT restate the type signature in prose or describe private implementation details. Specifically, it:

- Opens with a one-sentence summary of what the symbol does for the caller, in the present tense. Longer explanation goes in `@remarks`.
- Documents every parameter and type parameter with `@param` and `@typeParam`, giving intent and any constraint, unit, or accepted range rather than the type. A `@param` MAY be left out only when the name already fixes the meaning exactly.
- Documents the returned value with `@returns`, including the states a caller must handle, such as an empty collection, `null`, or a failed result. Leave it out for `void`.
- Gives one `@throws` per error type, with the condition that triggers it. A symbol that reports failure by return value instead says so where a reader would expect a throw.
- Gives `@defaultValue` to an optional parameter or property whose default a caller cannot infer.
- Carries `@deprecated` with the replacement and, when known, the version that removes it.
- Documents each public member of a class or interface with its own TSDoc. A bare member is an incomplete API.
- Prefers `{@link OtherSymbol}` to repeating another symbol's contract.

`@example` blocks are encouraged for any non-obvious usage; each MUST be realistic and compile against the current API.

An export needed only to compose entrypoints SHOULD stay out of public entrypoints. When one cannot, mark it `@internal`.

When a change touches a package's `exports`, its exported symbols, or its observable behavior or failures, it updates the TSDoc and every README or document describing them in the same change. oxlint checks the structure of the tags. Whether every export is documented, and how well, is a review responsibility unless the project enforces it.

Haku publishes no package, so these rules apply as for a private package. Public Rust items carry rustdoc, and a public function returning `Result` documents its failure conditions in an `# Errors` section.

`src/bindings.ts` is generated from Rust by `pnpm bindings`. Never edit it.

## Formatting, linting, and type checking

- Code MUST pass `pnpm verify`. [Workspace](../workspace.md#root-scripts) lists what it runs; while working, `pnpm format:check`, `pnpm lint:check`, and `pnpm typecheck` run its parts on their own.
- oxfmt formats code and JSON, and Prettier formats Markdown. Do not fight the formatter's output.
- Lint rules are enforceable project policy. Change the rule, or add a scoped override with its reason recorded, rather than ignoring it broadly. A local suppression comment says why it is there.

Haku has a second language, Rust, in `tauri/`. `rustfmt` formats it and `clippy` lints it, failing on any warning, as `pnpm verify` runs them. A local `#[allow(...)]` carries a comment saying why.

## Rust

- File and module names are snake_case; the kebab-case rule above is for TypeScript.
- Keep `browser.rs` and `model/` free of Tauri types. Logic that cannot be tested without a window is logic in the wrong module.
- Any command that drives a webview must be `async`. See [Architecture § Threading](../architecture.md#threading) for why.
- Every command guards with `ensure_chrome` before it does anything else. See [Security guidelines § Commands](security.md#commands).

## React

- Components hold no browser state. Read it from the store through `@ipc/hooks` and send intents through `commands` from `@ipc/commands`, never directly from `@bindings`, so failures are reported.
- Anything drawn over page content must call `useOverlay`, or it will render correctly and be unclickable. See [Chrome layering](../specs/chrome-layering.md#registering-an-overlay).
- A gesture that must stay smooth — dragging a tab — writes to the DOM through a ref during the gesture and commits to state on release, rather than re-rendering per frame.
- Notify through `feedback()` from `features/feedback`. Never use `alert`, `confirm` or `prompt`.
- Every user-facing string goes through `t()` from `@shared/i18n`. Haku ships English only today; routing strings through it from the start is what keeps adding locales from becoming a rewrite.

## Styling

- Use a `@codenhub/styles` component when one fits. Use Tailwind utilities for what it does not cover, over the palette's tokens; do not introduce colours of Haku's own.
- `.btn` and the other component classes are Tailwind utilities themselves. Two utilities setting the same property on one element resolve by Tailwind's sort order, not by intent, so show or hide a component from a wrapper instead of adding `hidden` to it.
- Size icons with the icon package's own classes (`ic-xs` … `ic-xl`). Its rules are unlayered, so Tailwind size utilities lose to them.
- Join conditional classes with `cx` from `@shared/class-names`.
- Draw every menu, popover and panel over the page with `haku-overlay`, and the rows in one with `haku-row`, both defined in `src/styles/index.css`. The recipe leaves padding and row height out: a menu or a panel adds `p-1`, a form `p-2`; menu rows are `min-h-6`, panel rows `min-h-8`. One recipe is what keeps popovers that do the same job from drifting apart.
- Prefer soft, flat or bare surfaces to outlined ones. Group with a soft fill one step from the ground (`.soft.edgeless` on a card or field), and separate with spacing. Draw a border, line or divider only where leaving it out is worse: where nothing else separates two things that must be told apart, or over page content, where a menu can cast no shadow and a border is what separates it from an unknown page beneath it ([Chrome layering](../specs/chrome-layering.md)). Outlines on every card, row and field add noise that competes with the content; Haku's look is monochrome and quiet, and edges are the loudest thing in it.

## Test names

A test is named after the behavior it checks, as a sentence, such as `it("names no reason when the engine gave none")`, rather than a camelCase identifier. Where Rust tests live is in the [testing specification](../specs/tests.md#rust).
