# Documentation

**Status:** IMPLEMENTED
**Last updated:** 2026-05-26

This document covers how the documentation in this project is structured, maintained, and how it should be interpreted, including this document itself.

## Structure

Every document MUST have the required fields at the top of the document.
Documents MAY include optional fields when they add clarity.

- **Status:** How reliable and trustworthy it should be considered:
  - DRAFT: Work in progress, use as reference.
  - APPROVED: The agreed source of truth. Future work MUST follow it. Existing code may still be non-compliant and should be treated as legacy until updated.
  - IMPLEMENTED: The agreed source of truth and the current implementation is expected to comply with it. New exceptions require updating this document or documenting the exception.
- **Last updated:** The date the document was last updated.
- **Scope:** Optional. What the document governs, when the title alone is not enough.

## Source of truth

Documentation MUST be updated in the same change when behavior, architecture, conventions, or project decisions change.

Truth priority is:

1. APPROVED or IMPLEMENTED documentation.
2. DRAFT documentation.
3. Existing code.

When APPROVED or IMPLEMENTED documentation conflicts with code, the documentation describes the intended direction and the code should be treated as legacy unless the document is outdated.

When APPROVED or IMPLEMENTED documents conflict with each other, both documents are invalid as sources of truth. Move both to DRAFT and only treat them seriously again after the conflict is resolved.

Prefer updating existing documents over creating new overlapping ones. Prefer updating documentation before changing code, so the intended direction is clear before implementation follows.

## What belongs here

Use `.docs/` for durable project knowledge:

- Architecture decisions.
- Implementation guidelines.
- Long-term conventions.
- Feature specs.
- Source-of-truth decisions.

Do not use `.docs/` for temporary notes, TODO lists, or information better expressed in code comments.

Plans and similar temporary documents MAY live in `.docs/plans/`. This directory is git-ignored and should stay that way because these documents are short-lived planning aids, not long-term project documentation.
