---
status: IMPLEMENTED
last_updated: 2026-10-23
scope: Index of the project documentation.
---

# Documentation

`docs/` holds durable project knowledge. [Contributing](../CONTRIBUTING.md) contains the contributor workflow; [agent instructions](../AGENTS.md) contain agent behavior guidance. [Documentation guidelines](guidelines/documentation.md) define how these documents are written, what their status means, and how they change.

Start with [Architecture](architecture.md).

## Overview

| Document                          | Contains                                                     |
| --------------------------------- | ------------------------------------------------------------ |
| [Architecture](architecture.md)   | What goes where and why. Start here.                         |
| [First release](first-release.md) | Release scope and the structural decisions features rest on. |

## Repository references

| Document              | Contains                                                                                                                 |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| [Tooling](tooling.md) | Scripts, what `pnpm verify` runs, git hooks, and generated files.                                                        |
| [Assets](assets.md)   | Brand artwork, fonts and app icon sources under `assets/`, the icons generated into `tauri/icons/`, and how to use them. |
| [CI](ci.md)           | Pinned toolchain, the verification jobs, and action pinning.                                                             |

## Guidelines

| Document                                     | Contains                                                                                              |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| [Code](guidelines/code.md)                   | Naming, structure, TypeScript, Rust, React and styling conventions; testing.                          |
| [Documentation](guidelines/documentation.md) | Metadata, status and authority, recording decisions, exceptions, and layout.                          |
| [Brand](guidelines/brand.md)                 | Name, logo, wordmark, colours and typefaces: how to use them, and why.                                |
| [Security](guidelines/security.md)           | Keeping web pages out of the application, in code: capabilities, commands, injected scripts, dialogs. |

## Specifications

| Document                                      | Contains                                                |
| --------------------------------------------- | ------------------------------------------------------- |
| [Webview pool](specs/webview-pool.md)         | How tabs share webviews; discarding, pinning, eviction. |
| [Tab optimization](specs/tab-optimization.md) | Which tabs keep a webview, and why; previews on reload. |
| [Chrome layering](specs/chrome-layering.md)   | How the interface renders above page content.           |
| [Page observation](specs/page-observation.md) | How page URLs become tab history; page dialogs.         |
| [Shortcuts](specs/shortcuts.md)               | The keymap, native key handling, reopening closed tabs. |
| [Extensions](specs/extensions.md)             | Installing unpacked extensions, the toolbar, popups.    |
| [Windows and popups](specs/new-windows.md)    | Browser windows, popups, and links that open them.      |
| [Tests](specs/tests.md)                       | Test categories, locations, tooling, and coverage.      |
| [Errors](specs/errors.md)                     | How failures are represented.                           |
| [Crash recovery](specs/crash-recovery.md)     | Crashed pages, a crashed interface, a failed engine.    |
| [Settings page](specs/settings-page.md)       | Sections, search and the row pattern of settings.       |
| [Context menus](specs/context-menus.md)       | Page and interface menus; the menu component.           |
