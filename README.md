# Haku

A lightweight desktop browser built on Tauri v2, with a Rust backend and a React interface.

Haku's distinguishing idea is that **tabs and webviews are not the same thing**. A small, configurable pool of real webviews — one by default — is shared between any number of tabs, so memory stays close to flat as tabs accumulate. Tabs you are not looking at are suspended and reload when you return to them; tabs you pin stay loaded.

## Status

Early. The foundation is in place: tabs, the webview pool, pinned tabs, per-tab history, light/dark theme, persistent sessions and history.

Windows only for now. The macOS and Linux native layers are stubs behind a shared interface.

## Running it

```bash
pnpm install
pnpm tauri dev
```

## Contributing

Start with [AGENTS.md](AGENTS.md), then the source of truth in [`docs/`](docs/).

Run `pnpm verify` before opening a pull request.
