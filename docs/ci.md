---
status: IMPLEMENTED
last_updated: 2026-10-06
scope: Continuous integration, the pinned toolchain, and the checks that report on pull requests.
---

# Continuous integration

`.github/workflows/ci.yml` verifies every pull request and every push to `main`. It runs the same scripts a contributor runs locally, so a green run means `pnpm verify` passed rather than that some CI-only approximation of it did.

## Toolchain

Local and CI runs resolve the same versions, each declared once:

| Where                             | Declares                                              |
| --------------------------------- | ----------------------------------------------------- |
| `package.json` `engines`          | The Node and pnpm range every run must satisfy        |
| `package.json` `packageManager`   | The exact pnpm version pnpm installs itself           |
| `.nvmrc`                          | The exact Node version, read by `nvm` and CI          |
| `.npmrc` `engine-strict`          | Makes an install outside `engines` fail               |
| `rust-toolchain.toml`             | The exact Rust version and its `clippy` and `rustfmt` |
| `tauri/Cargo.toml` `rust-version` | The oldest Rust the crate declares it builds with     |

CI reads those same files rather than repeating a version: `pnpm/action-setup` takes pnpm from `packageManager`, `actions/setup-node` takes Node from `.nvmrc`, and `rustup toolchain install` takes Rust from `rust-toolchain.toml`. A version is raised in one place, and a machine that cannot satisfy it is told at install time instead of failing later in a way nobody can reproduce.

`rust-toolchain.toml` sits at the repository root rather than in `tauri/` because rustup resolves it from the working directory, and the root scripts run cargo from the root with `--manifest-path`.

`engine-strict` is deliberate friction. A patch-level difference rarely matters, and the one time it does, the failure looks like a bug in the change under review rather than a difference in the runtime.

## Jobs

| Job    | Runner           | Runs                                                           |
| ------ | ---------------- | -------------------------------------------------------------- |
| `Web`  | `ubuntu-latest`  | `pnpm verify:web`                                              |
| `Rust` | `windows-latest` | `pnpm verify:rust`, then `pnpm build` and `pnpm check:release` |

Both jobs run on every pull request and every push to `main`, in parallel. There is no change-based selection: the repository is one application, and a check that skips half of it on a pull request is a check that lets the other half reach `main` unverified.

The split follows what each half needs. The interface and its tooling are platform-independent, so they run on the fastest, cheapest runner and report in a few minutes. Rust runs on Windows because Windows is the one platform with a native layer: the macOS and Linux layers in `platform/` are stubs, so a Linux build would compile code nothing runs, and it would also need WebKitGTK's system libraries before it built at all. Rust is verified where Haku runs. The rejected alternative was one Windows job for everything, which makes the interface's feedback wait behind a Rust build.

The `Rust` job also sets up Node and the dependencies, because `bindings:check` is a Node script and the release check compiles the interface bundle into the binary.

`check:release` compiles the release profile, which a debug build never sees. It exists for one known trap: `open_devtools` exists in a release build only through Tauri's `devtools` feature, so dropping the feature breaks `pnpm tauri build` while every debug check still passes. Removing the feature makes this step fail with `no method named open_devtools`. It is not part of `pnpm verify` because a release compile takes minutes on a cold cache; run it locally when changing `tauri/Cargo.toml` features.

Rust build output is cached by `Swatinem/rust-cache`, keyed on the toolchain, `Cargo.lock`, and the job's `CARGO_*` environment. A cold Tauri build is the slowest thing in a run by far, and the cache is what keeps a pull request's run short. On the first run, with an empty cache, the `Rust` job took 9 minutes; the same job warm took under 3, most of it setup and cache transfer. GitHub shares a cache only with the branch that saved it and with `main`, so a new branch starts from `main`'s cache, and a cold run comes back after a toolchain bump or a week without runs.

The `Rust` job builds debug and test profiles with `debug = "line-tables-only"`, set through `CARGO_PROFILE_*_DEBUG`. Compiler errors, clippy findings, and test failures read the same, and a panic's backtrace keeps its file and line, which `RUST_BACKTRACE=1` makes it always print. What is dropped is the variable and type information only a debugger reads, which nobody attaches to a CI run. The gain is modest, because clippy and the release check emit no debug information either way and only the test build changes: on a cold run the job went from 9m00s to 8m34s, the test build from 1m52s to 1m41s, and the cache from 784 MiB to 736 MiB. It stays because it costs nothing a CI run uses. Local builds keep Cargo's defaults, where a debugger is useful. Turning debug information off entirely was rejected: it could save a little more only on the test build, and costs backtraces their file and line.

Runs for the same pull request cancel each other, because only the newest push is worth a verdict. Runs on `main` never cancel: each merge is the authoritative check of that commit.

Workspace setup — pnpm, Node, and a frozen-lockfile install — lives in `.github/actions/setup`, so the two jobs cannot drift apart. `--locked` on every cargo script is the Rust half of the same rule: a `Cargo.lock` that disagrees with `Cargo.toml` fails rather than being rewritten in CI.

## Changing the workflow

Prefer adding a script to `package.json` over adding a step to the workflow. A step only CI can run is a step nobody can reproduce before pushing, and that is how CI turns into a second build system. [Tooling](tooling.md) describes the scripts.

### Pinning third-party actions

Every action from another repository is referenced by full commit SHA, with the release it corresponds to in a trailing comment:

```yaml
uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0
```

A tag is a mutable pointer. `@v5` resolves to whatever its owner last pointed it at, so a compromised or simply retagged release changes what runs here without any change landing in this repository. A SHA cannot move, which turns an action upgrade into a reviewable diff. The comment carries the human-readable version, because a bare SHA says nothing about how far behind it is.

The rule is all or nothing on purpose. Pinning some actions and not others is worse than pinning none: a reader cannot tell a reference that was reviewed from one that was missed. Adding an action means resolving its SHA in the same change.

`./.github/actions/setup` is deliberately not pinned. It is a path in this repository, already versioned by the commit under test.

Nothing updates these automatically. That is the cost of the rule, accepted rather than overlooked: upgrades are manual. Resolve the new SHA with `gh` and update the comment alongside it:

```sh
gh api repos/actions/checkout/commits/v5.1.0 --jq .sha
```

## Branch protection

The workflow reports; it does not block on its own. Requiring the `Web` and `Rust` checks before a merge into `main` is a repository setting on GitHub, not something a file here can enforce. It is recorded here so the intended configuration is reviewable even though the setting is not.

## Not covered yet

- **Releases.** Nothing builds, signs, or publishes installers. `pnpm tauri build` runs from a maintainer's machine.
- **Interface tests in a real webview.** Vitest runs the interface under jsdom; nothing drives the built app end to end.
