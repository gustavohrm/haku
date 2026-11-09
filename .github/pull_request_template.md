## What and why

<!-- What the change does for someone reading the log later, and why it is needed. -->

Fixes #

## Checks

- [ ] The delivery gate `CONTRIBUTING.md` names passes, `pnpm verify` by default, or the skipped steps are listed here
- [ ] Documentation affected by behavior, API, or convention changes is updated in this pull request

- [ ] `docs/architecture.md` is updated if the architecture changed
- [ ] `src/bindings.ts` is regenerated (`pnpm bindings`) if the IPC boundary changed
- [ ] `pnpm check:release` passes if `tauri/Cargo.toml` features changed
- [ ] A new IPC command, event, capability, or permission is named in the description and tested ([security guidelines](https://github.com/gustavohrm/haku/blob/main/docs/guidelines/security.md))

## Security

- [ ] No secret, token, or personal data can reach a log, an error, a snapshot, or a response other than the one that shows a new secret to its creator
- [ ] New untrusted input is validated where it enters, with a bounded size
- [ ] A new dependency, permission, or externally reachable surface is named in the description
