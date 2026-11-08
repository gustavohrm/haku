# Template conventions

This repository started from a shared project template. This directory records how its files relate to that template, so the template's improvements can reach this project and this project's improvements can reach the template. It is read by people and by agents; keep it in the project.

The template is brand-agnostic. It carries structure, tooling, and the general guidelines; a project brings its own name, brand, and domain rules.

## File classes

`manifest.json` assigns every file the template ships to one class:

| Class       | Meaning                                                                                                                                                                                      |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `canonical` | The template owns the whole file. A project does not edit it locally; it changes it in the template and syncs.                                                                               |
| `blocks`    | The template owns the file except its project blocks. Text inside a project block belongs to the project and survives a sync; text outside it is the template's.                             |
| `seed`      | The template provides a starting point, copied once. The project owns it from then on. A sync may suggest a template change to a seed file, but never applies one without the project's say. |

A file the manifest does not list belongs to the project.

`manifest.json#template` names the template repository as `owner/repo`, the one a sync fetches from.

The frontmatter values of a `blocks` document belong to the project, as if the frontmatter were a block: `status`, `last_updated`, and `scope` describe this project's copy, such as whether its code complies and when it last changed, so a sync keeps the project's values. The fields themselves are set by the [documentation guidelines](../docs/guidelines/documentation.md).

## Project blocks

A project block opens and closes with a marker in the file's own comment syntax:

```md
<!-- project:start -->

Project-specific text.

<!-- project:end -->
```

```sh
# project:start
# project:end
```

```jsonc
// project:start
// project:end
```

Plain JSON has no comments and cannot hold a block, so a JSON file that needs one is either read as JSONC by its tool, as `.oxlintrc.json` and `.oxfmtrc.json` are, or written in a format with comments, as `.prettierrc.json5` is. In a list or an object, the block opens it, before the template's entries: oxfmt removes the trailing comma after the last entry, so a block after it would need an edit outside the block to add the first project entry. A later duplicate key wins, so a block that opens an object adds keys and does not change the template's.

A block in the template holds placeholder guidance for what the project puts there, or default content the project keeps or replaces, such as the `push` trigger in `ci.yml`. A sync keeps the project's text in every block, so a later change to a block's default does not reach a project on its own. A file may have several blocks; they do not nest. A block the project leaves empty stays in the file, so a later sync knows where project text goes.

## Placeholders

Seed files and project blocks use these placeholders, replaced when a project is created:

| Placeholder       | Replaced with                                         | Example              |
| ----------------- | ----------------------------------------------------- | -------------------- |
| `{{name}}`        | The project's display name.                           | `Acme Notes`         |
| `{{slug}}`        | The repository and root package name.                 | `acme-notes`         |
| `{{scope}}`       | The npm scope of workspace packages, without the `@`. | `acme-notes`         |
| `{{description}}` | One sentence saying what the project is.              | `A note-taking app.` |
| `{{owner}}`       | The GitHub owner, a user or organization.             | `acme`               |
| `{{holder}}`      | The copyright holder.                                 | `Acme`               |
| `{{year}}`        | The year the project was created.                     | `2026`               |

A placeholder left in a file after creation is a bug.

## Exceptions

A project that deliberately departs from a `canonical` or `blocks` file, or does without one, lists it in `manifest.json#exceptions` with the reason, so the departure is a recorded decision rather than drift:

```json
"exceptions": {
  "tsconfig.base.json": "The packages extend the root tsconfig.json; moving to the template's layout is tracked in #123."
}
```

An exception excuses that file's differences from the template, including its absence. It does not excuse a placeholder left behind, unbalanced markers, or a `files` map that disagrees with the template's. A `seed` file needs no exception: the project already owns it. An exception whose file matches the template again fails the check, so it is removed once the reason is gone.

## Checking

`node .template/check.mjs` compares this project with the template at `synced` and changes nothing. It fails on:

- a `files` map that differs from the template's at `synced`, or a manifest of the wrong shape;
- a `synced` commit outside the history of the template's default branch, which is where a sync takes it from; a commit that only this repository has proves nothing about the template, so `synced` is set to the template commit the project last synced with;
- a `canonical` or `blocks` file that is missing;
- a `canonical` file that differs from the template's, including its mode, as for a hook that lost its executable bit;
- a `blocks` file whose text outside its project blocks differs, or whose markers are unbalanced;
- a placeholder left in a `seed` or `blocks` file;
- `core.hooksPath` not set to `.githooks`;
- an exception that is no longer needed.

It also reports how many commits the template is ahead of `synced`, without failing on it: a template change reaches the project when someone syncs, not when CI notices it. It fetches the template, so it needs the network; it adds no remote, ref, or `FETCH_HEAD`. With `synced` still `null`, as in the template itself, it checks only the manifest, the files, and the markers. The `Verify` job runs it whenever CI runs: on pull requests and merge-queue checks, and on pushes to `main` unless the project removed that trigger.

A difference from the template has one of three resolutions: sync, when the template changed; propose the change to the template, when the project improved a template-owned text; or list the file in `exceptions`, when the project departs from it on purpose. The other failures, a malformed manifest, broken markers, a placeholder, `core.hooksPath`, or an exception no longer needed, are fixed in the project.

## Syncing

`manifest.json#synced` records the template commit this project last synced with. A sync takes the `files` map from the template's current manifest, keeping this project's `template`, `synced`, and `exceptions`, so a file the template added or reclassified gets its new class. It then compares the template at that commit with the template now, applies the changes to `canonical` files and outside the blocks of `blocks` files, proposes changes to `seed` files, and updates `synced`. The comparison against the last synced commit, rather than against the project's own files, is what tells a template change apart from a deliberate project difference.

A project that improves a `canonical` file or the template-owned part of a `blocks` file proposes the improvement to the template instead of keeping a local edit.

### How to sync

Start on a branch, with a clean working tree and a passing `node .template/check.mjs`; a failure there is a difference to resolve before the sync, or the sync mixes it with the template's changes. `<synced>` is `manifest.json#synced`, and `<template>` is `manifest.json#template`.

1. Fetch the template, without adding a remote, and look at what changed:

   ```sh
   git fetch https://github.com/<template> HEAD
   git diff --stat <synced> FETCH_HEAD
   ```

2. Keep a copy of the current `files` map, then copy the `files` map of `git show FETCH_HEAD:.template/manifest.json` into this manifest, keeping `template`, `synced`, and `exceptions`. A file whose class changed is handled under its new class. For one that became `canonical`, the project's changes to it are considered for proposing upstream, then either the file takes the template's version, `git checkout FETCH_HEAD -- <path>`, or the project keeps its version and lists the file in `exceptions` with the reason; one that became `blocks` takes the template's text outside its blocks, with the project's text moved into them. A file the old map lists and the new one does not was removed or renamed by the template.
3. Apply the `canonical` and `blocks` changes with a three-way merge, taking the paths from the old map and the new one, so a file the template removed is deleted too:

   ```sh
   git diff --full-index --binary <synced> FETCH_HEAD -- <canonical and blocks paths> | git apply --3way
   ```

   A conflict in a `canonical` file is a local edit: the template's version wins, after the local change is considered for proposing upstream. A conflict in a `blocks` file keeps the text inside every project block exactly as it was and takes the template's text outside them. A clean merge needs the same care: where the project kept a block's old default, a change to that default applies without a conflict and overwrites the project's text, so compare each block with `git diff HEAD -- <blocks paths>` and restore any block the patch changed. The check cannot see this; it compares only the text outside the blocks. A placeholder the new template text brings is filled with the project's value. A file listed in `exceptions` is left out of the diff, and its exception reviewed against what the template changed.

4. Review `git diff <synced> FETCH_HEAD -- <seed paths>` file by file and apply only the changes the project wants. `pnpm-lock.yaml` is never patched; `pnpm install` regenerates it after a change to `package.json` or `pnpm-workspace.yaml`. A `seed` file the template removed belongs to the project, which decides whether to delete it.
5. Set `synced` to `git rev-parse FETCH_HEAD`, then run `pnpm install`, `node .template/check.mjs`, and `pnpm verify`, and commit. The commit message names the template commit, and the pull request lists the conflicts and how each was resolved, and the `seed` changes taken and declined.
