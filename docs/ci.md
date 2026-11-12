---
status: IMPLEMENTED
last_updated: 2026-11-12
scope: Continuous integration, the pinned toolchain, and the checks that report on pull requests.
---

# Continuous integration

`.github/workflows/ci.yml` verifies every pull request, every merge-queue check, and, unless the project requires a merge queue, every push to `main`. It runs the same script a contributor runs locally, `pnpm verify`, so a green run means `verify` passed rather than that some CI-only approximation of it did. [Workspace](workspace.md) says what each script guarantees.

## Toolchain

Local and CI runs resolve the same versions, each declared once:

| Where                                | Declares                                        |
| ------------------------------------ | ----------------------------------------------- |
| `package.json` `engines`             | The Node.js and pnpm range every run must meet  |
| `package.json` `packageManager`      | The exact pnpm version pnpm installs itself     |
| `.nvmrc`                             | The exact Node.js version, read by `nvm` and CI |
| `pnpm-workspace.yaml` `engineStrict` | Makes an install outside `engines` fail         |

CI reads those same files rather than repeating a version: `pnpm/action-setup` takes pnpm from `packageManager`, and `actions/setup-node` takes Node.js from `.nvmrc`. A version is raised in one place, and a machine that cannot satisfy it is told at install time instead of failing later in a way nobody can reproduce.

`engineStrict` is deliberate friction. A patch-level difference rarely matters, and the one time it does, the failure looks like a bug in the change under review rather than a difference in the runtime.

Workspace setup, pnpm, Node.js and a `--frozen-lockfile` install, lives in the composite action `.github/actions/setup`, so every job that needs the workspace sets it up the same way. A lockfile that disagrees with the manifests fails the install rather than being rewritten in CI.

### Dependency installs

`pnpm-workspace.yaml` holds the pnpm settings that govern installs, locally and in CI alike:

- `blockExoticSubdeps: true` refuses a transitive dependency that resolves to a git repository or a tarball URL instead of the registry. A direct dependency's source is written in `package.json`, where a reviewer sees it; a transitive one would arrive through another package's manifest unseen.
- `trustPolicy: no-downgrade` refuses a version whose publish-time trust evidence, such as provenance or trusted publishing, is weaker than that of an earlier version of the same package. A release that drops its provenance is a common sign of a compromised publish.
- `trustPolicyIgnoreAfter: 10080` is an age in minutes, seven days, after which a version is no longer held to the trust policy. It stops a long-published version that predates provenance from being refused as a downgrade, while a fresh release, the one a compromise would be, is still checked. When one recent version must be let through, `trustPolicyExclude` names that exact version, with the reason in a comment and the condition for removing it.
- `allowBuilds` lists the only dependencies whose install scripts run. It starts empty in the template: the template's tools ship their native binaries as optional dependencies and need none. A dependency that needs one, such as `esbuild`, gets an entry in the same change, with the reason.
- pnpm's default `minimumReleaseAge` prefers a version published at least a day ago, so a compromised release is usually caught and pulled before it is chosen here. It is not a hard gate: the workspace does not set `minimumReleaseAge`, so pnpm's default non-strict mode applies, and when the only version that matches a range or an exact pin is younger than a day, pnpm installs it anyway. Setting `minimumReleaseAge` explicitly, in `pnpm-workspace.yaml` or in a contributor's own pnpm configuration, makes it strict, so such an install fails instead; a version needed sooner then goes in `minimumReleaseAgeExclude`, by exact version, and leaves it once it is old enough.

## The workflow

| Job      | Runner           | Runs                                                                                                  |
| -------- | ---------------- | ----------------------------------------------------------------------------------------------------- |
| `Verify` | `ubuntu-latest`  | `pnpm verify:web`, then the `Working tree is clean` step                                              |
| `Rust`   | `windows-latest` | `pnpm verify:rust`, then `pnpm build` and `pnpm check:release`, then the `Working tree is clean` step |

`Verify` runs the web half of `pnpm verify` and `Rust` the other half; together they run all of it, in parallel. The repository is one application, so there is no change-based selection: a check that skips half of it on a pull request lets the other half reach `main` unverified.

By default there is no change-based selection: every run checks the whole workspace. A check that skips part of the workspace on a pull request lets that part reach `main` unverified, and a selection has to be maintained and can be wrong in ways a green run hides. A project whose full run grows too slow to wait for adds selection then, under three rules:

- **The whole workspace is checked before the merge lands.** A merge queue does that: it runs the full check on the merge result and drops a pull request that fails it. A push run on `main` only reports afterwards, once the failure is already there. Without a queue, the pull requests of codenhub/codenhub, the monorepo this rule comes from, went green on their own packages while 3 of 30 merges on 2026-11-03 failed on `main`, in packages those pull requests never touched ([codenhub/codenhub@702af904](https://github.com/codenhub/codenhub/commit/702af904)).
- **A change to `.github/` runs everything.** A workflow or the setup action belongs to no package, so a selection by changed package selects nothing, and the run passes without exercising the change it was opened for ([codenhub/codenhub@3d9bfe8a](https://github.com/codenhub/codenhub/commit/3d9bfe8a)).
- **The run asserts the ref it compares against.** A selection that compares against the base branch needs that ref in the checkout (`fetch-depth: 0`), and the job checks the ref exists before selecting: a tool that falls back to working-tree changes when the ref is missing would, in CI, check nothing and report success.

`Working tree is clean` fails when a check has left a tracked or untracked file changed. A check that writes has changed what it was checking, so it is reported rather than passing.

Runs for the same pull request cancel each other, because only the newest push is worth a verdict. A merge-queue run or a push to `main` is never cancelled to make way for a newer one: each is the authoritative check of a commit, so each is grouped by its commit rather than its branch. Grouping by branch would not hold that: a concurrency group holds one running and one pending run, and GitHub cancels the pending one when a third arrives, whatever `cancel-in-progress` says, so three quick pushes to `main` would leave the second without a verdict. The cost is that runs on `main` can run in parallel. The `merge_group` trigger never fires in a repository without a merge queue. A project that requires one removes the `push` trigger: the queue lands the exact commit it checked, so a push run would only repeat a passed check. The workflow's token is read-only (`permissions: contents: read`); a job that needs more asks for it on that job alone.

### The Rust job

The split follows what each half needs. The interface and its tooling are platform-independent, so they run on the fastest, cheapest runner and report in a few minutes. Rust runs on Windows because Windows is the one platform with a native layer: the macOS and Linux layers in `platform/` are stubs, so a Linux build would compile code nothing runs, and it would also need WebKitGTK's system libraries before it built at all. Rust is verified where Haku runs. The rejected alternative was one Windows job for everything, which makes the interface's feedback wait behind a Rust build.

Rust is pinned like Node and pnpm, and read by CI from the same files:

| Where                             | Declares                                              |
| --------------------------------- | ----------------------------------------------------- |
| `rust-toolchain.toml`             | The exact Rust version and its `clippy` and `rustfmt` |
| `tauri/Cargo.toml` `rust-version` | The oldest Rust the crate declares it builds with     |

`rustup toolchain install` takes Rust from `rust-toolchain.toml`. The file sits at the repository root rather than in `tauri/` because rustup resolves it from the working directory, and the root scripts run cargo from the root with `--manifest-path`. `--locked` on every cargo script is the Rust half of the frozen-lockfile rule: a `Cargo.lock` that disagrees with `Cargo.toml` fails rather than being rewritten in CI.

The `Rust` job also sets up Node and the dependencies, because `bindings:check` is a Node script and the release check compiles the interface bundle into the binary.

`check:release` compiles the release profile, which a debug build never sees. It exists for one known trap: `open_devtools` exists in a release build only through Tauri's `devtools` feature, so dropping the feature breaks `pnpm tauri build` while every debug check still passes. Removing the feature makes this step fail with `no method named open_devtools`. It is not part of `pnpm verify` because a release compile takes minutes on a cold cache; run it locally when changing `tauri/Cargo.toml` features.

Rust build output is cached by `Swatinem/rust-cache`, keyed on the toolchain, `Cargo.lock`, and the job's `CARGO_*` environment. A cold Tauri build is the slowest thing in a run by far, and the cache is what keeps a pull request's run short. On the first run, with an empty cache, the `Rust` job took 9 minutes; the same job warm took under 3, most of it setup and cache transfer. GitHub shares a cache only with the branch that saved it and with `main`, so a new branch starts from `main`'s cache, and a cold run comes back after a toolchain bump or a week without runs.

The `Rust` job builds debug and test profiles with `debug = "line-tables-only"`, set through `CARGO_PROFILE_*_DEBUG`. Compiler errors, clippy findings, and test failures read the same, and a panic's backtrace keeps its file and line, which `RUST_BACKTRACE=1` makes it always print. What is dropped is the variable and type information only a debugger reads, which nobody attaches to a CI run. The gain is modest, because clippy and the release check emit no debug information either way and only the test build changes: on a cold run the job went from 9m00s to 8m34s, the test build from 1m52s to 1m41s, and the cache from 784 MiB to 736 MiB. It stays because it costs nothing a CI run uses. Local builds keep full debug information, where a debugger is useful. `tauri/Cargo.toml` sets `strip = "debuginfo"` on the dev profile, but on Windows it changes nothing: the MSVC linker keeps debug information in a `.pdb` file beside the binary, which the setting does not touch, so a debugger and a panic's backtrace see the same either way. It strips the information from the binary only on macOS and Linux, where Haku does not run yet. Turning debug information off entirely was rejected: it could save a little more only on the test build, and costs backtraces their file and line.

### Cargo updates

`.github/dependabot.yml` also covers the Cargo dependencies, grouped like npm. The exception is the crates `tauri`, `tauri-build`, `windows` and `webview2-com`, which share a group of their own, majors included, so their available updates arrive in one pull request. The npm `@tauri-apps` packages are not in it. Haku's WebView2 code has to use the same `windows` and `webview2-com` versions as Tauri's runtime. The group only coordinates; it does not guarantee a build. A Tauri update can still arrive without the matching `windows` and `webview2-com` versions, and then needs them raised by hand in the same pull request.

## Changing the workflow

Prefer adding a script to `package.json` over adding a step to the workflow. A step only CI can run is a step nobody can reproduce before pushing, and that is how CI turns into a second build system.

### Pinning third-party actions

Every action from another repository is referenced by full commit SHA, with the release it corresponds to in a trailing comment:

```yaml
uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
```

A tag is a mutable pointer. `@v7` resolves to whatever its owner last pointed it at, so a compromised or simply retagged release changes what runs here without any change landing in this repository. A SHA cannot move, which turns an action upgrade into a reviewable diff. The comment carries the human-readable version, because a bare SHA says nothing about how far behind it is.

The rule is all or nothing on purpose. Pinning some actions and not others is worse than pinning none: a reader cannot tell a reference that was reviewed from one that was missed. Adding an action means resolving its SHA in the same change. The repository's Actions settings can enforce the rule, refusing any workflow that references another repository's action by anything but a full SHA; that setting is dashboard state, so turn it on when the repository is created.

`./.github/actions/setup` is deliberately not pinned. It is a path in this repository, already versioned by the commit under test.

For a hand upgrade, resolve the new SHA with `gh` and update the comment alongside it:

```sh
gh api repos/actions/checkout/commits/v7.0.1 --jq .sha
```

## Dependency updates

`.github/dependabot.yml` proposes weekly updates for the GitHub Actions, including the composite actions under `.github/actions/`, and for npm, where an update arrives as a catalog change in `pnpm-workspace.yaml` with the lockfile. It moves an action's SHA and its version comment together, so a pin and its label cannot drift apart. Every update pull request is labelled `type:chore`.

The groups take minor and patch updates only, so a week is one grouped pull request per ecosystem and directory. Dependabot groups each listed directory on its own, so the workflows and `.github/actions/*` can each get one: two pull requests for the actions in one week is the configuration working, not a duplicate. A major update arrives as a pull request of its own: it is a decision that may need code changes, and inside a group it would hold the routine updates back until it is dealt with.

`@types/node` never takes a major update from Dependabot. Node's types follow the Node.js release in `engines` and `.nvmrc`, and types for a newer Node accept APIs the pinned runtime does not have; they move by hand, with `.nvmrc`. A dependency held back for a known breakage gets an `ignore` entry with the reason and the condition for removing it.

Renovate does the same job, and was rejected: it runs as a GitHub App with a configuration of its own, while Dependabot is built into GitHub and already raises the repository's security updates.

## Branch protection

The workflow reports; it does not block on its own. What blocks is a branch protection rule or a ruleset on `main`, repository settings no file here can enforce. A ruleset does the same job with more control, and either works if nobody can bypass it. The recommended configuration:

| Setting                | Value                                 |
| ---------------------- | ------------------------------------- |
| Required status checks | `Verify`, from the GitHub Actions app |
| Pull request required  | Yes                                   |
| Enforced for admins    | Yes, with no bypass actors            |
| Force pushes, deletion | Refused                               |

Binding the checks to the GitHub Actions app stops any other integration from reporting a passing check under the same name. Admin enforcement is what makes the rest hold: without it, a maintainer's direct push to `main` is stopped only by the `pre-push` hook, which `--no-verify` skips.

Turn on "Automatically delete head branches" in the repository's general settings too; it is available on every plan. Deleting a merged branch is what makes GitHub retarget a pull request built on it to that branch's own base. With the branch left behind, the next pull request merges into it and never reaches `main`.

`main`'s protection differs from the recommendation in three ways:

| Setting                | Value                                                          |
| ---------------------- | -------------------------------------------------------------- |
| Required status checks | `Verify` and `Rust`, both from the GitHub Actions app          |
| Pull request required  | Yes, with no approving review, since one person maintains Haku |
| Up-to-date branches    | Not required                                                   |

`Rust` is required because it is half of `pnpm verify`. Up-to-date branches are not required: Haku has one contributor, and rebasing every branch before each merge would buy little. The web job was named `Web` before the template adoption; the required check on GitHub has to name `Verify` now, or every pull request waits on a check that never reports.

Admin enforcement also covers a weakness of the `pre-push` hook on Windows: git for Windows lets a push through when the hook is killed by a signal (`.githooks/AGENTS.md`). The cost is that the narrow exception in `CONTRIBUTING.md` for a direct commit to `main` means lifting the protection for that push and restoring it straight after.

## Other workflows

The workflow above verifies; it does not deploy, release or publish. A project that does adds a workflow of its own and documents it here.

- **Releases.** Nothing builds, signs, or publishes installers. `pnpm tauri build` runs from a maintainer's machine.
- **Interface tests in a real webview.** Vitest runs the interface under jsdom; nothing drives the built app end to end.
