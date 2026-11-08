# Agent instructions: `.githooks/`

These hooks are the template's. A fix to one belongs in the template, so every project that uses them gets it.

## Traps

- An edited hook does not run when tried from a worktree the Claude desktop app created; the main checkout's copy runs instead. The app writes an absolute `core.hooksPath` pointing at the main checkout into the worktree's `config.worktree`, which `git config --show-origin --get-all core.hooksPath` shows. Prefer `git -c core.hooksPath="$(pwd)/.githooks" <command>` to exercise the worktree's copy. Held on git 2.53.0 for Windows with the desktop app of October 2026: a push of `claude/agent-branch-naming-03bee5` went through while the worktree's `pre-push` already refused that name.
- On Git for Windows, a hook that is killed by a signal counts as passed: git proceeds and exits 0, so a rejected commit or push goes through with no message. The usual trigger is SIGPIPE, from piping git's output into something that stops reading early, such as `git commit ... 2>&1 | head -1`; a hook writing its rejection then dies mid-message. Each hook ignores SIGPIPE (`trap "" PIPE`), which covers that case only; a hook killed by SIGTERM or SIGKILL, from a timeout or an external kill, still passes. A hook that merely exits 141 is rejected as it should be. Prefer running git without such a pipe, or `| tail`, which reads to the end. Held on git 2.53.0.windows.2, reproduced in a throwaway repository on 2026-10-06 with a hook killing itself by each of the three signals; untested on Linux and macOS. See codenhub/codenhub#229.
- A hook loses its executable bit when it is added from a Windows checkout, where `core.fileMode` is false, and git on Linux and macOS then skips it with a "hook was ignored because it's not set as executable" hint. Prefer `git add --chmod=+x .githooks/<hook>` for a new hook, and check with `git ls-files -s .githooks`, which shows `100755` for each hook. Held on git 2.53.0.windows.2, checked in a throwaway repository on 2026-11-02: `git add` of a new hook recorded `100644`, and `git add --chmod=+x` recorded `100755`.

<!-- project:start -->

<!-- project:end -->
