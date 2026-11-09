# The pre-commit checks for Node.js and pnpm, sourced by `.githooks/pre-commit`,
# which says what each name below provides.
#
# oxfmt formats every staged file it understands and skips the rest, so the hook
# formats the same files `pnpm format:check` checks, including formats oxfmt
# gains in a later version; `ignorePatterns` in `.oxfmtrc.json` keeps it off
# Markdown. Prettier formats Markdown and skips anything `.prettierignore` lists;
# oxlint lints code. Rust goes to rustfmt, as `format:fix` and `format:check`
# in `package.json` do: rustfmt finds `tauri/rustfmt.toml` from each file's
# directory, but takes the crate's edition only as a flag. oxfmt does not read
# `.rs`, so it passes over those files.

# Any file with an extension: oxfmt decides which ones it formats.
PROFILE_EXTENSIONS='[^./]+'
PROFILE_FIX_HINT='Run `pnpm format:fix` and `pnpm lint:fix`'

code_extensions='cjs|cts|js|jsx|mjs|mts|ts|tsx'
rust_edition=2021

# oxfmt and oxlint exit non-zero when none of the files they are handed is one
# they read, whether ignored by their config or, for oxfmt, of a format it does
# not know; `--no-error-on-unmatched-pattern` turns that back into a pass, since
# committing such a file is not a failure.
profile_fix() {
  run_on "$1" pnpm exec oxfmt --no-error-on-unmatched-pattern --
  on_ext "$1" rs rustfmt --edition "$rust_edition" --
  on_ext "$1" md pnpm exec prettier --write --log-level warn --
  on_ext "$1" "$code_extensions" pnpm exec oxlint --fix --no-error-on-unmatched-pattern --
}

profile_check() {
  run_on "$1" pnpm exec oxfmt --check --no-error-on-unmatched-pattern --
  on_ext "$1" rs rustfmt --check --edition "$rust_edition" --
  on_ext "$1" md pnpm exec prettier --check --log-level warn --
  on_ext "$1" "$code_extensions" pnpm exec oxlint --deny-warnings --no-error-on-unmatched-pattern --
}
