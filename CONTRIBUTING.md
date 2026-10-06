# Contributing

This document defines the contributor workflow, from a working tree through validation and review. It applies to maintainers, contributors, and AI agents.

[Documentation](docs/README.md) maps the architecture, guidelines, and specifications. [Tooling](docs/tooling.md) describes the commands below. [Agent instructions](AGENTS.md) contain agent-specific behavior and context guidance.

## Setup

Haku builds on Windows. You need:

- Node and pnpm at the versions the repository pins. With `nvm`, `nvm use` reads `.nvmrc`; pnpm installs the version `packageManager` names.
- Rust through `rustup`. `rust-toolchain.toml` pins the version, and rustup installs it, with `clippy` and `rustfmt`, the first time cargo runs here.
- [Tauri's Windows prerequisites](https://tauri.app/start/prerequisites/): the Microsoft C++ Build Tools and the WebView2 runtime.

Then:

```sh
nvm use
pnpm install
pnpm tauri dev
```

The toolchain is pinned, and an install outside it fails rather than warns. `pnpm install` also points git at `.githooks/` through `core.hooksPath`, so the hooks described below start working after the first install and not before. [CI](docs/ci.md) covers why the versions are pinned where they are.

## Changes

- Keep each change within its requested scope and preserve unrelated work.
- Applicable guidelines, specifications, and APPROVED or IMPLEMENTED documentation govern the change. Existing code may lag those contracts; conflicts and exceptions are resolved under the [documentation guidelines](docs/guidelines/documentation.md).
- Update affected documentation in the same change when behavior, architecture, conventions, or decisions change. Each contract has one owning document; references point to that owner rather than repeat its requirements.
- Change the Rust, then run `pnpm bindings`, when the IPC boundary changes. `src/bindings.ts` is generated, and `pnpm verify` fails while it is stale.
- Do not commit secrets, build artifacts, or unrelated changes.

## Issues

Defects and requests are tracked as GitHub issues, opened through the bug and feature forms. A vulnerability is the exception: report it privately as [`SECURITY.md`](SECURITY.md) describes.

Each issue carries one label from each group that applies:

| Group     | Labels                            | Says                                 |
| --------- | --------------------------------- | ------------------------------------ |
| `type:`   | `bug`, `feature`, `docs`, `chore` | What kind of work it is              |
| `status:` | `needs-triage`, `blocked`         | Why it is not moving, when it is not |

`.github/labels.json` holds the list. `pnpm labels` creates or updates them on GitHub through `gh`, and is run after editing the list. It never deletes a label; one that is no longer listed is reported for a maintainer to remove by hand.

### A defect found mid-task

A defect hit while working on something else is filed and worked around, not fixed on the spot. Fixing it in place mixes two subjects in one branch, for the reason the [commit](#atomic-commits) and [pull request](#pull-requests) rules already reject.

File it with what triage needs:

```sh
gh issue create --title "<what is wrong>" --label "type:bug,status:needs-triage" --body "<what happened, what was expected, and the workaround>"
```

Then mark the workaround where it lives, naming the issue in full so the reference is unambiguous wherever it is read:

```ts
// Workaround for gustavohrm/haku#123: <what it works around>.
```

Once the fix lands, searching for `gustavohrm/haku#123` finds every workaround to remove. A defect with no workaround blocks the task; say so, and the fix goes first, in its own pull request.

A defect in a `@codenhub` package is filed in [codenhub/codenhub](https://github.com/codenhub/codenhub/issues) instead, with its `found-in:external` label, and the workaround names that issue: `// Workaround for codenhub/codenhub#123: ...`.

Opening an issue is outward-facing. An agent drafts it and asks before filing, as it does before pushing.

## Branches

Work happens on a branch. Do not commit to `main`.

`main` is the branch CI verifies on every push, so a commit that lands there directly is one nobody reviewed. A pre-push hook refuses to push to it.

The exception is real but narrow: it takes an explicit request from a maintainer, in the moment, for that specific commit. An agent must ask and be told yes. Neither a general instruction to "just fix it" nor a previous approval carries over to the next commit.

Name the branch `<type>/<slug>`, where `<type>` is the [commit type](#type) of the work and `<slug>` is kebab-case. A version in the slug keeps its dots.

```
feat/tab-search
fix/slot-eviction-order
docs/page-observation-dialogs
chore/repo-foundation
```

Every pushed branch follows the pattern, whoever or whatever created it. A tool that names its own branches, such as `claude/agent-branch-naming-03bee5` or `codex/fix-build-order`, does not make an exception: rename the branch with `git branch -m <type>/<slug>` before its first push. The `pre-push` hook refuses any other name.

## Commits

Commits follow [Conventional Commits](https://www.conventionalcommits.org):

```
<type>(<scope>)!: <subject>

<body>

<trailers>
```

A `commit-msg` hook checks the shape of the subject line. It cannot check whether the message is honest, which is the part that matters.

### Type

One of `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, `style`, `test`.

### Scope

The area the change lands in, and optional. Use the name the code already uses for it: an interface feature folder, such as `tabs` or `toolbar`; a Rust module, such as `browser`, `webview`, or `platform`; `ipc` for the boundary between them; `tauri` for the backend as a whole; or a repository area, such as `docs` or `ci`. Leave it out when the change spans several.

### Subject

Imperative mood, lowercase, no trailing period: "add", not "added" or "Adds". Aim for 50 characters and stay under 72, which the hook enforces.

Describe what the change does for someone reading the log later, not which files moved. `fix(webview): observe a slot before its first navigation` says what broke and what now happens; `fix(webview): update mod.rs` says nothing a diff would not.

Append `!` after the scope for a change that breaks a contract — an IPC command or event, or the format of persisted settings, sessions, or history — and say what breaks in the body.

### Body

Optional. Write one when the reason for the change is not obvious from the subject, and use it for why over what — the diff already carries the what.

### Atomic commits

One commit does one thing. A reviewer should be able to read the subject and know what is in the commit before opening it.

Split by intent, not by file count. Moving a function and changing its behavior are two commits even when they touch one file; renaming a symbol across twenty files is one commit. If a subject needs "and" to be accurate, that is usually two commits.

Formatting churn, unrelated fixes, and drive-by refactors are their own commits or their own pull request. Never bundle them into a behavior change: they make the real change unreviewable.

### Co-authorship

A commit an AI agent wrote or substantially shaped MUST carry a `Co-authored-by` trailer naming the **model**, not the tool or harness it ran in:

```
Co-authored-by: Claude Opus 5 <noreply@anthropic.com>
```

`Claude Opus 5`, not `Claude Code`; the model is what produced the change, and it is what someone auditing the history needs to know. Use the model vendor's no-reply address when it publishes one.

The human directing the work stays the commit author. The trailer is an addition to authorship, never a replacement for it.

## Validation

Run scripts from the repository root. During development, narrow checks to what changed — `pnpm test <file>`, `pnpm test:rust -- <module>`. Before delivering a change, run the whole gate:

```sh
pnpm verify
```

Report any step that was skipped or failed. After changing features in `tauri/Cargo.toml`, also run `pnpm check:release`, which CI runs on every pull request.

## Pull requests

Every change reaches `main` through a pull request, and CI verifies it on Ubuntu and Windows.

Pushing a branch and opening a pull request are outward-facing actions. An agent asks first and does neither on its own initiative.

Keep a pull request to one subject. A branch that fixes a bug and also restructures a doc is two pull requests, for the same reason a commit that does both is two commits.

A pull request that resolves an issue says `Fixes #123` in its description, so merging it closes the issue.

## Hooks

Three hooks run locally, all from `.githooks/`:

| Hook         | Checks                                                       |
| ------------ | ------------------------------------------------------------ |
| `pre-commit` | Formats and lints the staged files, re-staging what it fixed |
| `commit-msg` | The subject line shape described above                       |
| `pre-push`   | Refuses a direct push to `main`, and a misnamed branch       |

`--no-verify` bypasses them. It is for the commit that genuinely has to land unfixed, and an agent that reaches for it MUST say so in the same breath rather than quietly routing around a failing check. [Tooling](docs/tooling.md#git-hooks) describes what each hook does and why it does no more than that.
