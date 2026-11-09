---
status: IMPLEMENTED
last_updated: 2026-11-08
scope: How the `package.json` scripts are implemented, what `pnpm verify` runs, the TypeScript and Vitest configuration, git hooks, and generated files.
---

# Tooling

Every check is a `package.json` script, run from the repository root. [Workspace](workspace.md) is the contract the script names and their guarantees follow; this document says how Haku implements them. Cargo scripts reach the crate with `--manifest-path tauri/Cargo.toml`, so nothing needs to be run from inside `tauri/`. CI runs these same scripts and nothing else; [CI](ci.md) describes how.

There is no task runner. Haku is one application, and a dozen scripts chained with `&&` are easier to read and reproduce than a tool that would have to be learned, versioned, and trusted first.

## One package at the root

The shared template lays a project out as a pnpm workspace of `apps/*` and `packages/*`. Haku keeps one package at the repository root instead, with `packages: [.]` in `pnpm-workspace.yaml`. Moving the app into `apps/haku` would rewrite paths that the rest of the repository relies on: the Tauri crate in `tauri/` and its `tauri.conf.json`, which runs `pnpm dev` and reads `../dist`; the Rust test that writes `src/bindings.ts` and the script that checks it; the Rust paths in CI; and the path aliases and the links throughout `docs/`. Haku has no second package to share code with, so the layout would buy nothing yet. The decision is worth revisiting when a second app or a shared package appears. Everything else follows the template: the root scripts, the TypeScript solution, the catalog, and the shared configurations.

## Scripts

| Script                | Runs                                                          |
| --------------------- | ------------------------------------------------------------- |
| `pnpm tauri dev`      | The app, with the interface served by Vite                    |
| `pnpm dev`            | Vite alone, on port 1420                                      |
| `pnpm build`          | `tsc -b`, then the interface bundle into `dist/`              |
| `pnpm verify`         | `verify:web`, then `verify:rust`                              |
| `pnpm verify:web`     | Format, lint, the interface bundle, type check, and Vitest    |
| `pnpm verify:rust`    | rustfmt, clippy, the bindings drift check, and `cargo test`   |
| `pnpm format:check`   | oxfmt, Prettier for Markdown, and rustfmt, without writing    |
| `pnpm format:fix`     | The same three formatters, writing                            |
| `pnpm format`         | `format:check`                                                |
| `pnpm lint:check`     | oxlint and clippy, both failing on any warning                |
| `pnpm lint:fix`       | oxlint's fixes                                                |
| `pnpm typecheck`      | `tsc -b` over the TypeScript solution                         |
| `pnpm test`           | Vitest, once                                                  |
| `pnpm test:watch`     | Vitest in watch mode                                          |
| `pnpm test:coverage`  | Vitest with a V8 coverage report                              |
| `pnpm test:rust`      | `cargo test`                                                  |
| `pnpm bindings`       | Regenerates `src/bindings.ts` from Rust                       |
| `pnpm bindings:check` | Fails when `src/bindings.ts` is stale                         |
| `pnpm check:release`  | `cargo check` on the release profile                          |
| `pnpm labels`         | Creates or updates the GitHub labels in `.github/labels.json` |

`format:check`, `lint:check`, and `verify` each have a `:web` and a `:rust` half, so CI can run each half on the runner it needs. The halves are what the whole runs; there is no step only one of them knows about.

`pnpm format` duplicates `pnpm format:check` on purpose. When a name matches no script, pnpm runs the executable of that name from `PATH` instead, and on Windows `format` is the system disk formatter.

Every cargo script passes `--locked`, the Rust counterpart to `pnpm install --frozen-lockfile`: a `Cargo.lock` that disagrees with `Cargo.toml` fails rather than being rewritten. After changing a dependency, run a plain `cargo build` in `tauri/` once to update the lock file, and commit it.

## Verification

`pnpm verify` is the gate every change passes before delivery. It runs, stopping at the first failure:

1. `format:check:web` — oxfmt over code, CSS, HTML and JSON; Prettier over Markdown.
2. `lint:check:web` — oxlint with `--deny-warnings`.
3. `build` — `tsc -b`, then `vite build`, the production bundle, which is where a missing import or a broken virtual module shows up.
4. `typecheck` — `tsc -b`, which after `build` finds its incremental state current and returns at once.
5. `test` — Vitest.
6. `format:check:rust` — rustfmt.
7. `lint:check:rust` — clippy over every target, with warnings denied.
8. `bindings:check` — the drift check described below.
9. `test:rust` — `cargo test`.

The web half follows the order [Workspace](workspace.md#root-scripts) sets. `build` type-checks before bundling because `tauri build` runs `pnpm build` alone, and a release should not bundle code that does not type-check. The web half runs first because it reports in seconds, while the Rust half may compile for minutes.

`check:release` is not part of `verify`. A release compile on a cold cache takes minutes and guards one trap, a missing Tauri `devtools` feature; CI runs it on every pull request, and it is worth running locally when changing features in `tauri/Cargo.toml`.

## Formatters and linters

| Language                    | Formatter | Linter | Configuration                          |
| --------------------------- | --------- | ------ | -------------------------------------- |
| TypeScript, CSS, HTML, JSON | oxfmt     | oxlint | `.oxfmtrc.json`, `.oxlintrc.json`      |
| Markdown                    | Prettier  | —      | `.prettierrc.json5`, `.prettierignore` |
| Rust                        | rustfmt   | clippy | `tauri/rustfmt.toml`                   |

oxfmt does not format Markdown; Prettier does, and it is the one holding the `proseWrap: never` rule [Documentation guidelines](guidelines/documentation.md) set. Prettier is given an explicit `**/*.md` target set, so it touches nothing else.

rustfmt runs at width 120, the same as oxfmt's `printWidth`, so both languages wrap at the same column. clippy runs its default lint set with every warning denied. A lint that is wrong for one place is allowed there with `#[allow(...)]` and a comment saying why; a lint that is wrong everywhere is a change to this document.

`src/bindings.ts` is generated, so oxfmt and oxlint list it in their `ignorePatterns`, and the `pre-commit` hook, which passes `--no-error-on-unmatched-pattern`, leaves it as its generator wrote it.

## TypeScript and Vitest

`tsconfig.json` is a solution with no files of its own. `pnpm typecheck` runs `tsc -b` on it, which checks the two projects it references:

| Project              | Checks                                                                                                                                                                 |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tsconfig.root.json` | The template's: the root config files and the `.githooks/` scripts, by Node's rules.                                                                                   |
| `tsconfig.app.json`  | The interface in `src/`: DOM and `vite/client` types, React JSX, `exactOptionalPropertyTypes`, and the `@app`, `@bindings`, `@ipc`, `@features` and `@shared` aliases. |

Both extend `tsconfig.base.json`, the template's shared compiler settings, and keep their incremental state under `node_modules/.cache/`. The aliases are declared three times, in `tsconfig.app.json`, `vite.config.ts` and `vitest.config.ts`, because each tool resolves imports itself.

`vitest.config.ts` runs `src/**/*.test.ts` and `src/**/*.test.tsx` under jsdom, with V8 coverage over `src/`. [Tests](specs/tests.md) owns what the suites cover.

## Generated files

`src/bindings.ts` is the TypeScript side of the IPC boundary, generated from Rust by a cargo test, `ipc::tests::export_bindings`. A test rather than a build script, so that generation is explicit and an unrelated build cannot rewrite the interface's types. `pnpm bindings` runs it.

`pnpm bindings:check` regenerates the file and fails if the result differs from what was there before. It also fails unless cargo reports exactly one test run, so a renamed `export_bindings` test cannot pass the check having regenerated nothing. Comparing against the file as it was before the run, rather than against git, is what makes it work on a working tree with uncommitted Rust changes. A stale file is left regenerated, which is the content to review and commit.

`pnpm test:rust` runs the same test among the others and so also rewrites the file. `verify` runs `bindings:check` first, so the check sees the file as committed.

## Git hooks

The three hooks in `.githooks/` came from the template; [Contributing](../CONTRIBUTING.md#hooks) lists what each checks, and [Workspace](workspace.md#git-hooks) records why each does that much and no more.

`pre-commit` is a stack-neutral dispatcher that sources `.githooks/profile.sh` for its checks: oxfmt, Prettier for Markdown, and oxlint, as the template's Node profile has them, and rustfmt, with the crate's edition, over staged `.rs` files, which oxfmt cannot format. Clippy stays in `pnpm verify`, for the same reason type checking does.

Each hook ignores SIGPIPE, and `.githooks/AGENTS.md` records why. The cost is that tools a hook runs inherit the ignored signal, so with output piped through something that closes early, a passing hook can fail on a write error. Failing closed is the safer direction.

A fix to a hook belongs in the template, so every project that uses it gets it.
