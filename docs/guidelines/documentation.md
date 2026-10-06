---
status: IMPLEMENTED
last_updated: 2026-10-06
scope: Durable project documentation under `docs/`, including this document.
---

# Documentation guidelines

This document defines how the documentation in this repository is structured, maintained, and interpreted, including this document itself.

## Required header

Every document in `docs/` MUST start with YAML frontmatter containing these fields, in this order:

- `status`: how reliable and authoritative the document is.
- `last_updated`: date of the last meaningful content change in `YYYY-MM-DD` format.

Documents SHOULD include `scope` when the title alone does not make ownership clear.

Allowed statuses:

- `DRAFT`: Work in progress. Use as context, not as binding source of truth.
- `APPROVED`: Agreed source of truth. Future work MUST follow it. Existing code may be non-compliant and should be treated as legacy until updated.
- `IMPLEMENTED`: Agreed source of truth and current implementation is expected to comply. New exceptions MUST be documented or the document MUST be updated.
- `SUPERSEDED`: Temporary. Being replaced by a refactor that lands across more than one change, while the code it describes is still live. The document MUST name its replacement in its first paragraph and MUST be deleted in the change that completes the replacement. It is never a way to keep a retired document around.

```yaml
---
status: APPROVED
last_updated: 2026-07-15
scope: Area governed by this document.
---
```

The header is YAML rather than bold lines in the body because a tool can read it without parsing prose, and because Prettier, which keeps Markdown unwrapped, would otherwise join the lines into one.

## Formatting

Markdown in this repository is not hard-wrapped. Write each paragraph and list item as a single line and let the editor wrap it on screen. Markdown renders a lone newline inside a paragraph as a space, so wrapped and unwrapped source read identically once rendered, and leaving prose unwrapped keeps an edit to one word from reflowing a whole paragraph in the diff.

`pnpm format:check` enforces this: it runs Prettier over every Markdown file with `proseWrap` set to `never`, and `pnpm verify` and the `pre-commit` hook run the same check. The rule is the formatter's to keep — do not hand-wrap prose to a column, and do not set a `max_line_length` for Markdown in editor configuration. `.editorconfig` sets it to `off`, which Prettier reads as an unlimited print width; that is also what keeps a wide table aligned rather than collapsed.

## Source of truth

Documentation MUST be updated in the same change when behavior, architecture, conventions, or project decisions change.

APPROVED and IMPLEMENTED documents record decisions. A decision binds work while it stands, and changing it is legitimate work: the document is the current answer, not a final one. Judge work against a document on two separate questions, and keep the answers apart:

- **Conformance:** does the work match the recorded decision? Where it does not, the work is brought in line, or the departure is recorded under "Exceptions".
- **Merit:** is the recorded decision right? Where it is not, the decision and its document change, together with the code that follows them.

Matching a document does not make work correct, and a merit finding is not a code defect: it is a proposal to change a decision, made as one and under "Recording decisions".

Truth priority settles conformance, which is what work follows while a decision stands:

1. APPROVED or IMPLEMENTED documentation.
2. DRAFT documentation.
3. Existing code.

When APPROVED or IMPLEMENTED documentation conflicts with code, the documentation describes the intended direction and the code should be treated as legacy unless the document is outdated.

When APPROVED or IMPLEMENTED documents conflict with each other, the conflict MUST be resolved in the same change if practical. If not practical, move the conflicting documents to `DRAFT` and add a short note explaining the conflict.

Prefer updating existing documents over creating overlapping ones. Prefer updating documentation before changing code so intended direction is clear before implementation follows.

Delete a document once it no longer describes current or intended direction, in the same change that makes it stale. Git keeps its history; an outdated document kept in the tree only competes with the current one in search and review.

## Recording decisions

A document that settles a choice MUST say why. Where real alternatives existed, it SHOULD name the ones it rejected and, briefly, why each lost. A decision recorded without its reason can only be obeyed or overruled, not judged, and the next reader reopens it.

A change that reverses a recorded decision MUST name what the decision did not weigh: a concrete case it gets wrong, a measurement, or a constraint that has changed since. Preferring another option is not enough; without new evidence, the recorded decision stands. The reversing change records the old choice among the rejected alternatives, with the evidence that reversed it, so the decision is not reopened on arguments already heard.

## Exceptions

Exceptions to APPROVED or IMPLEMENTED documents MUST be explicit, scoped, and justified.

An exception MUST state:

- What rule is being bypassed.
- Where the exception applies.
- Why the exception is acceptable.
- Whether it is temporary or permanent.

Do not create broad exceptions for one-off cases. Prefer changing the rule when repeated exceptions show the rule is wrong. Record an exception next to the rule it bypasses.

## What belongs here

Use `docs/` for durable project knowledge:

- Architecture decisions.
- Implementation guidelines.
- Long-term conventions.
- Feature specs.
- Source-of-truth decisions.

Do not use `docs/` for temporary notes, TODO lists, or information better expressed in code comments. Traps — what goes wrong when trying something non-obvious — belong in a scoped `AGENTS.md`, as `AGENTS.md` describes.

Plans and similar temporary documents MAY live in `docs/plans/`. This directory is git-ignored and should stay that way because these files are short-lived planning aids, not durable documentation.

## Layout

`docs/` is organized by what a document is for:

- `docs/specs/`: contracts for one part of the product or the repository — how the webview pool behaves, how errors are represented, what tests must look like. A spec is written so compliance can be checked.
- `docs/guidelines/`: conventions contributors apply by judgment while working — how to write code, how to write documentation. A guideline MAY contain enforced rules, but its main job is to shape decisions no checklist fully captures.
- Root `docs/`: the documentation index, the architecture overview, release scope, and references for one repository area or process, such as `tooling.md` and `ci.md`.

When a new document could fit more than one place, file it by its main job: rules a part must satisfy go in `specs/`, how to work goes in `guidelines/`, and what exists and how to use it stays at the root. Do not add another folder until a group of documents fits none of these.

Every document is listed in `docs/README.md`. Adding, moving, or deleting one updates that index in the same change.
