---
status: IMPLEMENTED
last_updated: 2026-10-06
scope: The `package.json` scripts, what `pnpm verify` runs, git hooks, and generated files.
---

# Tooling

Every check is a `package.json` script, run from the repository root. Cargo scripts reach the crate with `--manifest-path tauri/Cargo.toml`, so nothing needs to be run from inside `tauri/`. CI runs these same scripts and nothing else; [CI](ci.md) describes how.

There is no task runner. Haku is one application, and a dozen scripts chained with `&&` are easier to read and reproduce than a tool that would have to be learned, versioned, and trusted first.

## Scripts

| Script                | Runs                                                          |
| --------------------- | ------------------------------------------------------------- |
| `pnpm tauri dev`      | The app, with the interface served by Vite                    |
| `pnpm dev`            | Vite alone, on port 1420                                      |
| `pnpm build`          | `tsc`, then the interface bundle into `dist/`                 |
| `pnpm verify`         | `verify:web`, then `verify:rust`                              |
| `pnpm verify:web`     | Format, lint, type check, Vitest, and the interface bundle    |
| `pnpm verify:rust`    | rustfmt, clippy, the bindings drift check, and `cargo test`   |
| `pnpm format:check`   | oxfmt, Prettier for Markdown, and rustfmt, without writing    |
| `pnpm format:fix`     | The same three formatters, writing                            |
| `pnpm format`         | `format:check`                                                |
| `pnpm lint:check`     | oxlint and clippy, both failing on any warning                |
| `pnpm lint:fix`       | oxlint's fixes                                                |
| `pnpm typecheck`      | `tsc`                                                         |
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
3. `typecheck` — `tsc`.
4. `test` — Vitest.
5. `vite build` — the production bundle, which is where a missing import or a broken virtual module shows up.
6. `format:check:rust` — rustfmt.
7. `lint:check:rust` — clippy over every target, with warnings denied.
8. `bindings:check` — the drift check described below.
9. `test:rust` — `cargo test`.

The order is by cost within each half: the cheapest step most likely to fail on a fresh change runs first. The web half runs first because it reports in seconds, while the Rust half may compile for minutes.

`check:release` is not part of `verify`. A release compile on a cold cache takes minutes and guards one trap, a missing Tauri `devtools` feature; CI runs it on every pull request, and it is worth running locally when changing features in `tauri/Cargo.toml`.

## Formatters and linters

| Language                    | Formatter | Linter | Configuration                         |
| --------------------------- | --------- | ------ | ------------------------------------- |
| TypeScript, CSS, HTML, JSON | oxfmt     | oxlint | `.oxfmtrc.json`, `.oxlintrc.json`     |
| Markdown                    | Prettier  | —      | `.prettierrc.json`, `.prettierignore` |
| Rust                        | rustfmt   | clippy | `tauri/rustfmt.toml`                  |

oxfmt does not format Markdown; Prettier does, and it is the one holding the `proseWrap: never` rule [Documentation guidelines](guidelines/documentation.md) set. Prettier is given an explicit `**/*.md` target set, so it touches nothing else.

rustfmt runs at width 120, the same as oxfmt's `printWidth`, so both languages wrap at the same column. clippy runs its default lint set with every warning denied. A lint that is wrong for one place is allowed there with `#[allow(...)]` and a comment saying why; a lint that is wrong everywhere is a change to this document.

`src/bindings.ts` is generated, so oxfmt, oxlint, and the `pre-commit` hook leave it as its generator wrote it.

## Generated files

`src/bindings.ts` is the TypeScript side of the IPC boundary, generated from Rust by a cargo test, `ipc::tests::export_bindings`. A test rather than a build script, so that generation is explicit and an unrelated build cannot rewrite the interface's types. `pnpm bindings` runs it.

`pnpm bindings:check` regenerates the file and fails if the result differs from what was there before. Comparing against the file as it was before the run, rather than against git, is what makes it work on a working tree with uncommitted Rust changes. A stale file is left regenerated, which is the content to review and commit.

`pnpm test:rust` runs the same test among the others and so also rewrites the file. `verify` runs `bindings:check` first, so the check sees the file as committed.

## Git hooks

Three hooks live in `.githooks/`, wired by `core.hooksPath`, which the `prepare` script sets on every `pnpm install`. That is a local git setting rather than a tracked one, so a fresh clone runs no hooks until it installs; doing it from `prepare` keeps a hook manager out of the dependencies. The setup step never fails an install: a tree without git reports and carries on.

| Hook         | Checks                                                           |
| ------------ | ---------------------------------------------------------------- |
| `pre-commit` | Formats and lints the staged files, re-staging what it fixed     |
| `commit-msg` | The Conventional Commits subject `CONTRIBUTING.md` describes     |
| `pre-push`   | Refuses a push to `main`, and a branch not named `<type>/<slug>` |

`pre-commit` runs oxfmt, Prettier, rustfmt, and oxlint over the staged files and nothing else. Type checking, clippy, and tests take minutes, and a hook that slow gets bypassed until it may as well not exist; `pnpm verify` is where they belong. It reads files from the working tree rather than from the index. A file that is only partly staged is never rewritten, because re-staging it would sweep in the parts deliberately left out of the commit; it is checked as it sits on disk, so a commit whose staged content is already clean can still fail.

`commit-msg` checks the subject line and nothing else: a known type, an optional lowercase scope, an imperative lowercase subject, no trailing period, 72 characters at most. No hook can tell whether a message describes the commit honestly, so that half stays with review. Merges, reverts, and `fixup!` markers pass unchecked, because git composed them.

`pre-push` reads the destination ref rather than the current branch, so it holds for a push from `main`, for an explicit refspec targeting it, and for a `--delete`. It checks the branch name because the name is often not a choice anyone made: an agent harness creates `claude/...` or `codex/...` before the agent has read a file. Deleting a remote branch passes whatever it is called, and tags are not checked.

Each hook ignores SIGPIPE. Git for Windows counts a hook killed by any signal as passed, and when the caller pipes git through something that closes early, such as `head`, a hook writing its rejection is killed by SIGPIPE mid-message. Ignoring the signal leaves the hook's own verdict in charge of its exit status. It covers that case only; `.githooks/AGENTS.md` records the rest. The cost is that tools a hook runs inherit the ignored signal, so with output piped that way a passing hook can fail on a write error. Failing closed is the safer direction.

`--no-verify` bypasses them: on `git commit` for the first two, on `git push` for the last. It is for the change that genuinely has to land unfixed, and using it is worth saying out loud.

The hooks and their messages come from the codenhub repository, where they were written; a fix to one is worth carrying to the other.
