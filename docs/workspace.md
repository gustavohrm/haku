---
status: IMPLEMENTED
last_updated: 2026-11-08
scope: The root script names and what each guarantees, the shared tool configurations, and how dependency versions are declared.
---

# Workspace

This document is the workspace's contract: the names of the root scripts and what each one guarantees, the configuration files every project shares, and how versions are declared. CI, the git hooks, and the agent instructions rely on it. How this project implements the scripts, narrows a run, and adds a package is in [Tooling](tooling.md).

## Root scripts

| Script               | Guarantees                                                                                                                                    |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm format:check`  | Every file is formatted: code, JSON and YAML by oxfmt, Markdown by Prettier. Reports, writes nothing.                                         |
| `pnpm format:fix`    | The same two formatters, rewriting files.                                                                                                     |
| `pnpm lint:check`    | oxlint passes over the repository, with warnings treated as errors.                                                                           |
| `pnpm lint:fix`      | oxlint, applying its safe fixes.                                                                                                              |
| `pnpm typecheck`     | Every package and the repository's own TypeScript type-check.                                                                                 |
| `pnpm test`          | Every package's and app's unit and integration suites pass.                                                                                   |
| `pnpm test:watch`    | The same, in watch mode.                                                                                                                      |
| `pnpm test:coverage` | The same, with a coverage report in `coverage/`.                                                                                              |
| `pnpm build`         | Every package that produces output builds, in dependency order.                                                                               |
| `pnpm verify`        | `format:check`, `lint:check`, `build`, `typecheck` and `test`, in that order, then any check the project adds, stopping at the first failure. |
| `pnpm prepare`       | Runs on install: points git at the committed hooks in `.githooks/`.                                                                           |

`pnpm verify` is what CI runs ([CI](ci.md)), so a green local `verify` predicts a green run. The order puts the cheapest checks first, so a formatting slip is reported in a second instead of after the tests. The build comes before type checking and tests because a package that imports another workspace package resolves it through that package's `exports`, which for a built package point at `dist/`: on a fresh clone, as in CI, a type check or a test run before the build cannot resolve the import and fails.

Keep these names as they are: CI, the git hooks and the agent instructions call them by name. A script that only some projects need, such as `dev` or an end-to-end suite, is the project's to add.

<!-- project:start -->

| Script                   | Guarantees                                                                               |
| ------------------------ | ---------------------------------------------------------------------------------------- |
| `pnpm dev`               | Vite alone, serving the interface on port 1420.                                          |
| `pnpm tauri dev`         | The app, with the interface served by `pnpm dev`.                                        |
| `pnpm preview`           | The built interface, served by Vite.                                                     |
| `pnpm format`            | The same as `format:check`, so a mistyped call cannot reach Windows' disk formatter.     |
| `pnpm format:check:web`  | `format:check` for everything but Rust.                                                  |
| `pnpm format:check:rust` | rustfmt over the crate in `tauri/`, writing nothing.                                     |
| `pnpm lint:check:web`    | oxlint, warnings denied.                                                                 |
| `pnpm lint:check:rust`   | clippy over every target, warnings denied.                                               |
| `pnpm test:rust`         | The crate's `cargo test`.                                                                |
| `pnpm bindings`          | Regenerates `src/bindings.ts` from Rust.                                                 |
| `pnpm bindings:check`    | Fails when `src/bindings.ts` is stale.                                                   |
| `pnpm check:release`     | The release profile compiles.                                                            |
| `pnpm verify:web`        | `format:check:web`, `lint:check:web`, `build`, `typecheck` and `test`, in that order.    |
| `pnpm verify:rust`       | `format:check:rust`, `lint:check:rust`, `bindings:check` and `test:rust`, in that order. |
| `pnpm labels`            | The GitHub labels match `.github/labels.json`.                                           |

`format:check`, `format:fix` and `lint:check` cover Rust too, through rustfmt and clippy. `pnpm verify` is `verify:web`, then `verify:rust`: the template's order for everything but Rust, then the Rust checks and `bindings:check`. Each half runs on the CI runner it needs ([CI](ci.md)), and together they run every step of the contract.
<!-- project:end -->

## Configuration

| File                  | Configures                                                                                                                                                                                                                                                                |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `package.json`        | The root scripts, the tooling as `devDependencies`, and the pinned Node.js and pnpm (`engines`, `packageManager`).                                                                                                                                                        |
| `pnpm-workspace.yaml` | The workspace globs, the catalog of every dependency version, the supply-chain rules for installs ([CI](ci.md#dependency-installs)), and `engineStrict`, so an install outside `engines` fails.                                                                           |
| `.nvmrc`              | The exact Node.js version, read by `nvm` and by CI.                                                                                                                                                                                                                       |
| `.editorconfig`       | Indentation, final newline, LF line endings, and no hard wrap in Markdown.                                                                                                                                                                                                |
| `.gitattributes`      | LF line endings in the repository and the working tree on every platform, and the binary file types.                                                                                                                                                                      |
| `.oxfmtrc.json`       | oxfmt: 120 columns, double quotes, semicolons, trailing commas, sorted imports and `package.json` keys. Markdown is excluded; Prettier owns it. Nothing else is listed: oxfmt already skips what `.gitignore` and `.prettierignore` list, and the lockfile.               |
| `.prettierrc.json5`   | Prettier, for Markdown only: `proseWrap: "never"`, so a paragraph is one line and a diff shows the sentence that changed.                                                                                                                                                 |
| `.prettierignore`     | Paths inside the Markdown set Prettier leaves alone: build output and caches.                                                                                                                                                                                             |
| `.oxlintrc.json`      | oxlint's plugins and rules. `no-console` allows `console.warn` and `console.error`, a library's deliberate diagnostics, and refuses the rest, such as a forgotten `console.log`, except in `.githooks/`, any `scripts/` directory and tests, where printing is the point. |

The TypeScript and Vitest configurations depend on how the project runs its checks; [Tooling](tooling.md) describes them.

<!-- project:start -->

| File                    | Configures                                                                                                                                                            |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm-workspace.yaml`   | `packages: [.]`: Haku is one package at the repository root, not a workspace of `apps/*` and `packages/*`. [Tooling](tooling.md#one-package-at-the-root) records why. |
| `rust-toolchain.toml`   | The exact Rust version and its `clippy` and `rustfmt`, at the root because rustup resolves it from the working directory.                                             |
| `tauri/rustfmt.toml`    | rustfmt: width 120, matching oxfmt.                                                                                                                                   |
| `tauri/Cargo.toml`      | The crate, its dependencies and Tauri features, and its `rust-version`.                                                                                               |
| `tauri/tauri.conf.json` | The app: its bundle, and the dev and build commands that run `pnpm dev` and `pnpm build`.                                                                             |
| `vite.config.ts`        | The interface bundle, the dev server ports, and the path aliases, which `tsconfig.app.json` and `vitest.config.ts` repeat.                                            |

<!-- project:end -->

### Two formatters

oxfmt formats everything it understands except Markdown; Prettier formats only Markdown. oxfmt is far faster and agrees with Prettier's output for code, but its Markdown support does not yet match Prettier's, and the documentation is where formatting churn costs the most review time. `.oxfmtrc.json` ignores `**/*.md`, and the scripts hand Prettier `**/*.md` explicitly, so neither formatter touches the other's files.

### Versions

Every third-party dependency version is declared once, in the catalog in `pnpm-workspace.yaml`, and referred to as `catalog:` in each `package.json`. The workspace therefore cannot hold two versions of one dependency by accident, and an upgrade is a one-line change that Dependabot can propose ([CI](ci.md#dependency-updates)). A workspace package depends on another as `workspace:*`, and a `peerDependencies` range says what a consumer may install, so neither is a catalog entry.

<!-- project:start -->

Rust dependencies are declared in `tauri/Cargo.toml` and locked in `tauri/Cargo.lock`; the catalog holds the npm ones only.
<!-- project:end -->

## Git hooks

[Contributing](../CONTRIBUTING.md#hooks) lists what each hook checks. This section records why each does that much and no more.

`pre-commit` formats and lints the staged files, fixes what it can, and re-stages the result. It runs those checks and no others: type checking and tests take minutes, and a hook that slow gets bypassed until it may as well not exist. `pnpm verify` is where the rest belongs.

`pre-commit` reads files from the working tree rather than from the index. For a file that is only partly staged that matters: the hook never rewrites it, because re-staging would sweep in the parts deliberately left out of the commit, and it checks the file as it sits on disk. A commit whose staged content is already clean can therefore still fail. The alternative, skipping such files, would let unformatted code through the one check meant to stop it.

`commit-msg` checks the subject line and nothing else. A body is optional, and no hook can tell whether a message describes the commit honestly, so the mechanical half is automated and the rest stays with review. Merges, reverts, and `fixup!`, `squash!`, and `amend!` markers pass unchecked: git composes those subjects, and rejecting a message nobody wrote would only train people to reach for `--no-verify` out of habit, which costs the checks that do matter. The 72-character limit is where practice already sat rather than a number from a style guide: replayed over 200 earlier commits of codenhub/codenhub, it rejected six, three merely long and three joining two changes with "and", which is the split the convention asks for anyway.

The hooks are wired by `core.hooksPath`, which the root `prepare` script sets on every install. That is a local git setting rather than a tracked one, so a fresh clone runs no hooks until something sets it; doing it from `prepare` keeps a hook manager out of the dependency list. The setup step never fails an install: a tree without git, or without permission to write its config, reports and carries on.

`pre-push` reads the destination ref rather than the branch you are standing on, so its refusal to push to `main` holds for a push from `main`, for an explicit refspec that targets it from elsewhere, and for a `--delete`. It also refuses a branch whose name is not `<type>/<slug>`. That check sits in a hook rather than in wording alone because the name is often not a choice anyone made: an agent harness creates `claude/...` or `codex/...` before the agent has read a file, and a rule about naming a branch does not read as applying to a branch that already has a name. Deleting a remote branch passes whatever it is called, since removing a misnamed branch is the cleanup, and tags are not checked.

`--no-verify` bypasses these: on `git commit` for the first two, on `git push` for the last. It is for the change that genuinely has to land unfixed, and it is worth saying out loud when it is used, since the whole point of a hook is that skipping one is visible.
