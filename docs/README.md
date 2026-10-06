---
status: IMPLEMENTED
last_updated: 2026-10-06
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

| Document              | Contains                                                          |
| --------------------- | ----------------------------------------------------------------- |
| [Tooling](tooling.md) | Scripts, what `pnpm verify` runs, git hooks, and generated files. |
| [CI](ci.md)           | Pinned toolchain, the verification jobs, and action pinning.      |

## Guidelines

| Document                                     | Contains                                                                     |
| -------------------------------------------- | ---------------------------------------------------------------------------- |
| [Code](guidelines/code.md)                   | Naming, structure, TypeScript, Rust, React and styling conventions; testing. |
| [Documentation](guidelines/documentation.md) | Metadata, status and authority, recording decisions, exceptions, and layout. |

## Specifications

| Document                                      | Contains                                                 |
| --------------------------------------------- | -------------------------------------------------------- |
| [Webview pool](specs/webview-pool.md)         | How tabs share webviews; discarding, pinning, eviction.  |
| [Tab optimization](specs/tab-optimization.md) | Approved, not built: which tabs keep a webview, and why. |
| [Chrome layering](specs/chrome-layering.md)   | How the interface renders above page content.            |
| [Page observation](specs/page-observation.md) | How page URLs become tab history; page dialogs.          |
| [Tests](specs/tests.md)                       | Test categories, locations, tooling, and coverage.       |
| [Errors](specs/errors.md)                     | How failures are represented.                            |
