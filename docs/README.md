---
status: IMPLEMENTED
last_updated: 2026-11-08
scope: Index of the project documentation.
---

# Documentation

`docs/` holds durable project knowledge. [Contributing](../CONTRIBUTING.md) contains the contributor workflow, and [agent instructions](../AGENTS.md) contain how agents work here. The [documentation guidelines](guidelines/documentation.md) define how these documents are written, what their status means, and how they change.

Every document in `docs/` is listed here. Adding, moving, or deleting one updates this index in the same change.

<!-- project:start -->

Start with [Architecture](architecture.md).

## Overview

| Document                          | Contains                                                     |
| --------------------------------- | ------------------------------------------------------------ |
| [Architecture](architecture.md)   | What goes where and why. Start here.                         |
| [First release](first-release.md) | Release scope and the structural decisions features rest on. |

<!-- project:end -->

## Guidelines

| Document                                     | Contains                                                                                        |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| [Code](guidelines/code.md)                   | Naming, design, TypeScript, async and errors, testing practices, and documentation in code.     |
| [Documentation](guidelines/documentation.md) | Metadata, status and authority, recording decisions, exceptions, layout, and the writing voice. |
| [Security](guidelines/security.md)           | Secrets, untrusted input, what stays out of logs, and reviewing a change.                       |

<!-- project:start -->

| Document                     | Contains                                                               |
| ---------------------------- | ---------------------------------------------------------------------- |
| [Brand](guidelines/brand.md) | Name, logo, wordmark, colours and typefaces: how to use them, and why. |

<!-- project:end -->

## Repository references

| Document                  | Contains                                                                                                         |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| [Workspace](workspace.md) | The root scripts and what each guarantees, the shared tool configurations, and dependency versions.              |
| [Tooling](tooling.md)     | How the scripts are implemented, the TypeScript and Vitest configuration, narrowing a run, and adding a package. |
| [CI](ci.md)               | The pinned toolchain, the verification jobs, and dependency updates.                                             |

<!-- project:start -->

| Document            | Contains                                                                                                                 |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| [Assets](assets.md) | Brand artwork, fonts and app icon sources under `assets/`, the icons generated into `tauri/icons/`, and how to use them. |

<!-- project:end -->

## Specifications

| Document                | Contains                                                          |
| ----------------------- | ----------------------------------------------------------------- |
| [Tests](specs/tests.md) | Test categories, locations, configuration, scripts, and coverage. |

<!-- project:start -->

| Document                                         | Contains                                                      |
| ------------------------------------------------ | ------------------------------------------------------------- |
| [Webview pool](specs/webview-pool.md)            | How tabs share webviews; discarding, pinning, eviction.       |
| [Split view](specs/split-view.md)                | Panes, focus, layout and session restoration.                 |
| [Tab optimization](specs/tab-optimization.md)    | Which tabs keep a webview, and why; previews on reload.       |
| [Chrome layering](specs/chrome-layering.md)      | How the interface renders above page content.                 |
| [Page observation](specs/page-observation.md)    | How page URLs become tab history; page dialogs.               |
| [Shortcuts](specs/shortcuts.md)                  | The keymap, rebinding, key handling, reopening closed tabs.   |
| [Extensions](specs/extensions.md)                | Installing unpacked extensions, the toolbar, popups.          |
| [Windows and popups](specs/new-windows.md)       | Browser windows, popups, and links that open them.            |
| [Errors](specs/errors.md)                        | How failures are represented.                                 |
| [Crash recovery](specs/crash-recovery.md)        | Crashed pages, a crashed interface, a failed engine.          |
| [Settings page](specs/settings-page.md)          | Sections, search and the row pattern of settings.             |
| [Appearance](specs/appearance.md)                | Tab, sidebar and bar layouts; spacing; floating bars, reveal. |
| [Context menus](specs/context-menus.md)          | Page and interface menus; the menu component.                 |
| [Fullscreen](specs/fullscreen.md)                | A page entering fullscreen; how it ends.                      |
| [Downloads](specs/downloads.md)                  | Following downloads, the webviews they hold, storage.         |
| [Find, print and save](specs/find-print-save.md) | The find bar; printing and saving a page.                     |
| [Permissions](specs/permissions.md)              | Permission prompts; stored decisions, private windows.        |
| [Zoom](specs/zoom.md)                            | Zooming pages; levels kept per site, shown and reset.         |
| [Bookmarks](specs/bookmarks.md)                  | Saving pages, folders, the bar, import and export.            |
| [History](specs/history.md)                      | What is recorded, rules, filters, deleting, storage.          |
| [Address bar](specs/address-bar.md)              | Inline and floating layouts, suggestions, search engines.     |

<!-- project:end -->
