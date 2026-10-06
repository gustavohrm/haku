# Agent instructions: `src/`

## Traps

- An overlay renders correctly and is completely unclickable when it is drawn over page content without calling `useOverlay`. The chrome sits above the page with a native input mask cut out of it, and only registered overlays are added back to the mask. Prefer `useOverlay` for anything over the page, and `feedback()` for toasts and dialogs, which `features/feedback` registers. [Chrome layering](../docs/specs/chrome-layering.md#registering-an-overlay) explains the mask.
- A failed command is silently dropped when it is called through `@bindings` directly. The generated commands resolve a failure to a value, and most call sites fire and forget. Prefer `@ipc/commands`, whose wrapper turns a failure into a toast.
- An edit to `bindings.ts` is lost on the next `pnpm bindings`, and fails `pnpm bindings:check` before that. The file is generated from Rust. Prefer changing the Rust in `tauri/src/ipc/` and regenerating.
- Every icon renders as a solid block when `@codenhub/icons` is imported from CSS. That resolves to the package's plain base sheet, which has no per-icon rules. Prefer `virtual:icons.css`, imported once in `main.tsx`. An icon class name that never appears as a literal string somewhere in `src/` is not generated either, so build names from a fixed set rather than by concatenation. Held on `@codenhub/icons` 0.2.
