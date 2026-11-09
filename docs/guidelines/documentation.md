---
status: IMPLEMENTED
last_updated: 2026-11-08
scope: Durable project documentation under `docs/`, including this document.
---

# Documentation guidelines

This document defines how the documentation in this repository is structured, maintained, and interpreted, including this document itself, and how any text a person reads here is written.

## Required header

Every document in `docs/` MUST start with YAML frontmatter containing `status` and `last_updated`, and `scope` unless the title alone makes it clear, in this order:

- `status`: how reliable and authoritative the document is.
- `last_updated`: the date of the last meaningful content change, in `YYYY-MM-DD` format.
- `scope`: what the document governs.

Allowed statuses:

| Status        | Meaning                                                                                                                                                                                                                                                                              |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `DRAFT`       | Work in progress. Use it as context, not as a binding source of truth.                                                                                                                                                                                                               |
| `APPROVED`    | The agreed source of truth. Future work MUST follow it. Existing code that does not is legacy until updated.                                                                                                                                                                         |
| `IMPLEMENTED` | The agreed source of truth, and the current implementation is expected to comply. A new exception MUST be documented, or the document MUST change.                                                                                                                                   |
| `SUPERSEDED`  | Temporary, while a replacement lands across more than one change and the code it describes is still live. It MUST name its replacement in its first paragraph and MUST be deleted in the change that completes the replacement. It is never a way to keep a retired document around. |

```yaml
---
status: APPROVED
last_updated: 2026-11-08
scope: Area governed by this document.
---
```

The header is YAML rather than bold lines in the body because a tool can read it without parsing prose, and because Prettier, which keeps Markdown unwrapped, would otherwise join the lines into one.

A file that is itself a template for consumer-facing output, such as a README template a generator copies, carries no frontmatter, so repository governance never leaks into what it produces. Repository status is also never package stability or consumer support metadata, and a tool MUST NOT read it as such.

## Formatting

Markdown in this repository is not hard-wrapped. Write each paragraph and list item as a single line and let the editor wrap it on screen. Markdown renders a lone newline inside a paragraph as a space, so wrapped and unwrapped source read identically once rendered, and leaving prose unwrapped keeps an edit to one word from reflowing a whole paragraph in the diff.

`pnpm format:check` enforces this: it runs Prettier over every Markdown file with `proseWrap` set to `never`, and `pnpm verify` and the pre-commit hook run the same check. The rule is the formatter's to keep: do not hand-wrap prose to a column, and do not give Markdown a line length in editor configuration. `.editorconfig` sets `max_line_length = off` for Markdown, which says the same to editors and is how Prettier reads an unlimited width. Third-party Markdown listed in `.prettierignore` is kept as received. Agent skills are formatted like the rest, but with their fenced examples left as written: an agent reads a skill as raw text, where an example's line breaks are part of what it shows.

## Source of truth

Documentation MUST be updated in the same change when behavior, architecture, conventions, or project decisions change.

APPROVED and IMPLEMENTED documents record decisions. A decision binds work while it stands, and changing it is legitimate work: the document is the current answer, not a final one. Judge work against a document on two separate questions, and keep the answers apart:

- **Conformance:** does the work match the recorded decision? Where it does not, the work is brought in line, or the departure is recorded under [Exceptions](#exceptions).
- **Merit:** is the recorded decision right? Where it is not, the decision and its document change, together with the code that follows them.

Matching a document does not make work correct, and a merit finding is not a code defect: it is a proposal to change a decision, made as one and under [Recording decisions](#recording-decisions).

Truth priority settles conformance, which is what work follows while a decision stands:

1. APPROVED or IMPLEMENTED documentation.
2. DRAFT documentation.
3. Existing code.

When APPROVED or IMPLEMENTED documentation conflicts with code, the documentation describes the intended direction and the code is legacy, unless the document is outdated.

When APPROVED or IMPLEMENTED documents conflict with each other, the conflict MUST be resolved in the same change if practical. If not, move the conflicting documents to `DRAFT` and add a short note explaining the conflict.

Each rule has one owning document; others link to it rather than repeat it. A rule stated twice drifts into two rules. Prefer updating an existing document to creating an overlapping one, and prefer updating documentation before changing code, so the intended direction is clear before the implementation follows.

Delete a document once it no longer describes current or intended direction, in the same change that makes it stale. Git keeps its history; an outdated document kept in the tree only competes with the current one in search and review.

## Recording decisions

A document that settles a choice MUST say why. Where real alternatives existed, it SHOULD name the ones it rejected and, briefly, why each lost. A decision recorded without its reason can only be obeyed or overruled, not judged, and the next reader reopens it.

A change that reverses a recorded decision MUST name what the decision did not weigh: a concrete case it gets wrong, a measurement, or a constraint that has changed since. Preferring another option is not enough; without new evidence, the recorded decision stands. The reversing change records the old choice among the rejected alternatives, with the evidence that reversed it, so the decision is not reopened on arguments already heard.

This applies to every document that uses this status model, wherever it lives in the repository.

## Exceptions

Exceptions to APPROVED or IMPLEMENTED documents MUST be explicit, scoped, and justified. An exception MUST state:

- What rule is being bypassed.
- Where the exception applies.
- Why the exception is acceptable.
- Whether it is temporary or permanent.

Record an exception next to the rule it bypasses, unless a dedicated register serves that scope better. Do not create broad exceptions for one-off cases. Repeated exceptions show the rule is wrong: change the rule.

## What belongs here

Use `docs/` for durable project knowledge:

- Architecture decisions.
- Implementation guidelines.
- Long-term conventions.
- Feature specs.
- Source-of-truth decisions.

Do not use `docs/` for temporary notes, TODO lists, or information better expressed in code comments. Traps, what goes wrong when trying something non-obvious in one directory, belong in a scoped `AGENTS.md` (see [Agent instructions](#agent-instructions)).

Documentation that concerns one workspace package only MAY live with that package, in its README or its own `docs/`. Root `docs/` holds what spans the repository.

Root `docs/` is not consumer documentation by default. A site, catalog, or other publishing tool MUST include root documents only through an explicit selection rule, never by treating all of `docs/` as public content.

Plans and similar temporary documents MAY live in `docs/plans/`. This directory is git-ignored and should stay that way: these are short-lived planning aids, not durable documentation.

## Layout

`docs/` is organized by what a document is for:

- `docs/specs/`: contracts for a deliverable, such as what tests must look like or how a feature behaves. A spec is written so compliance can be checked.
- `docs/guidelines/`: conventions contributors apply by judgment while working, such as how to write code or documentation. A guideline MAY contain enforced rules, but its main job is to shape decisions no checklist fully captures.
- `docs/guides/`: documentation for the people who use, run, or integrate with what the project ships, read outside the repository. Add it when the project has such readers.
- Root `docs/`: the documentation index and references for one repository area or process, such as `workspace.md`, `tooling.md`, `ci.md`, an architecture overview, and an optional `roadmap.md`.

When a new document could fit more than one place, file it by its main job: rules a deliverable must satisfy go in `specs/`, how to work goes in `guidelines/`, what a user of the product needs goes in `guides/`, and what exists and how to use it stays at the root. Do not add another folder until a group of documents fits none of these.

A roadmap, when the project keeps one, records durable direction: current focus, what is planned, what may come, and what was ruled out and why. It is not a task list or a replacement for issue tracking. Entries describe outcomes, not steps, and carry dates only for a real commitment or external constraint.

Every document is listed in [`docs/README.md`](../README.md). Adding, moving, or deleting one updates that index in the same change.

Haku's root `docs/` also holds [the architecture overview](../architecture.md), [the first release's scope](../first-release.md), and [the assets reference](../assets.md). Its specs cover the product's behavior, such as how the webview pool works and how errors are represented, as well as the repository's own contracts.

## Agent instructions

`AGENTS.md` files hold how to work (the root one) and the known traps of a directory (scoped ones). Rules and decisions do not go there; they belong in `docs/`, where they carry a status and a reason. The root `AGENTS.md` describes the trap format.

## Writing

### Explain, don't compress

The usual failure is a sentence that names five behaviors so each counts as mentioned. Unpack it. For each behavior, say what happens, why when the reason helps the reader predict related behavior, and what to do about it, with an example when it is not obvious.

Before, compliant but unusable:

> Invalid paths, duplicate patterns, query or hash content, and dot segments throw.

After:

> Paths are checked when you register them, so a mistake fails at startup instead of on some unlucky click. These all throw:
>
> - Two routes with the same shape. `/users/:id` and `/users/:name` match exactly the same URLs, so the second could never win. Keep one of them.
> - A query or hash in the pattern. `/search?q=:term` throws: routes match the path only. Read the query from the matched URL instead.
>
> …and so on for each case, with its fix.

Error and limitation sections get the same care. A reader there is usually stuck, so give the cause and the way out, not just the condition.

### Documents that teach

A guide, a README, or any page meant to teach what the project ships to someone new owes more than accuracy: it owes completeness, in an order a reader can follow.

**Start from an inventory.** Before writing, list what the document owes the reader: each entrypoint, option, and default; each capability, constraint, failure, known limitation, and workaround; each thing deliberately not done; and each security, accessibility, and performance responsibility left to the reader. Pull from the source, the tests, the internal docs, and the scoped `AGENTS.md` files. Tests are the most reliable list of edge cases a reader can hit, and a trap that a reader could fall into needs a reader-facing explanation too. Decide where each item is explained; an item without a home means the document is not finished. The inventory is a working aid and need not be committed.

**Tell it as a story.** A reader should be able to go through the pages in order and come out knowing the thing, the way a good tutorial teaches a tool rather than a manual listing its buttons.

- **Open with the problem.** Say what it is for and what it saves the reader from doing by hand, before any API appears.
- **Order by need, not by module.** Put first what a newcomer needs first, then what they reach for next. Source layout is the wrong order more often than not.
- **Carry a running example.** One small, realistic scenario that grows across pages connects them and shows how features combine. Add something on each page instead of starting over.
- **Common case first, edge cases after.** Start with the smallest example that works, then realistic use, then limits, failures, and workarounds. A reader who stops early has still learned something correct.
- **Hand off at the end.** Each page ends by saying where to go next and why.

### Voice

Documents that teach, such as guides, READMEs, and tutorials, use the voice below. Everything else a person reads to check or decide, such as specifications, guidelines, changelogs, issues, and pull request descriptions, is plain: precise and friendly, without the jokes. A reader of a specification is checking compliance, and a reader of a changelog is deciding whether to upgrade; neither is helped by a wink. The guardrails below hold for plain text too.

An earlier version of this section applied the playful voice to every document, changelogs and pull request descriptions included. It was narrowed because those are read to check or decide, where a joke costs the reader time and rewards nothing.

Write like a knowledgeable friend explaining something they built: precise, a little dry, and happy to grin about the odd parts. Humor lives in the connective prose, such as introductions, transitions, and asides, and in notes, tips, informational callouts, and anything else where nothing is at risk. There it rewards the reader without slowing them down.

The kind of thing that fits:

- "It needs a note? Then note it." — when an API does exactly what its name says.
- "Tokens expire, because security, duh." — teasing the domain, not the reader.
- "This is the boring part. It's also the part that pages you at 3 a.m., so stay with us."

Guardrails:

- **Aim jokes at the situation, the domain, or ourselves, never at the reader.** "Duh" about token expiry is fine; "duh" about someone not knowing something reads as contempt, and these documents assume a reader who does not know.
- **Keep the plain meaning recoverable.** Many readers are not native English speakers, and many read through a translator or an LLM. A pun can add to a sentence, but the sentence must still state the fact literally.
- **Keep it out of warnings, security notes, error explanations, and copyable code.** A reader in those places needs the fact, nothing else. Example code may use playful names in snippets meant only to be read; code meant to be pasted uses realistic names.
- **One wink, not a stand-up set.** A light touch every few paragraphs is enough. If a joke needs explaining or has to be forced in, cut it.
- **Leave existing tone to its owner.** A writer does not change the tone of existing text, adding jokes to it or sanding them off, without the approval of whoever owns that text. Fix what is wrong or unclear; keep the voice it was written in.

A project's brand guideline, when it has one, MAY refine this voice: move it toward formal or toward playful, extend it to other documents, add principles and words to use or avoid, and set the voice for surfaces this section does not cover, such as a website or marketing copy. Where the two differ on tone, the brand guideline wins for that project. The guardrails above hold either way. The [documentation index](../README.md) links it.
