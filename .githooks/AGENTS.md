# Agent instructions: `.githooks/`

## Traps

- An edited hook does not run when tried from a worktree the Claude desktop app created; the main checkout's copy runs instead. The app writes an absolute `core.hooksPath` pointing at the main checkout into the worktree's `config.worktree`, which `git config --show-origin --get-all core.hooksPath` shows. Prefer `git -c core.hooksPath="$(pwd)/.githooks" <command>` to exercise the worktree's copy. Held on git 2.53.0 for Windows with the desktop app of October 2026, observed in the codenhub repository these hooks come from: a push of `claude/agent-branch-naming-03bee5` went through while the worktree's `pre-push` already refused that name.
