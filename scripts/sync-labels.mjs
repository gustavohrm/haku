#!/usr/bin/env node
// Creates or updates the GitHub labels listed in `.github/labels.json`.
//
// Every label is written with `gh label create --force`, so a run is safe to
// repeat and updates colors and descriptions in place. It never deletes one:
// deleting a label strips it from every issue that carries it, so a label on
// GitHub that the list no longer has is reported for a maintainer to remove.
//
// The repository is resolved from this checkout and passed with `--repo`, so a
// `GH_REPO` exported for other work cannot send these labels elsewhere.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const LABELS_PATH = ".github/labels.json";

function gh(args) {
  const result = spawnSync("gh", args, { encoding: "utf8", shell: false });
  if (result.error !== undefined || result.status !== 0) {
    throw new Error(`gh ${args.join(" ")} failed: ${result.error?.message ?? result.stderr.trim()}`);
  }
  return result.stdout;
}

const repository = gh(["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]).trim();
const labels = JSON.parse(readFileSync(LABELS_PATH, "utf8"));

for (const label of labels) {
  gh([
    "label",
    "create",
    label.name,
    "--repo",
    repository,
    "--color",
    label.color,
    "--description",
    label.description,
    "--force",
  ]);
  process.stdout.write(`Synced ${label.name}\n`);
}

const listed = new Set(labels.map((label) => label.name));
const existing = JSON.parse(gh(["label", "list", "--repo", repository, "--limit", "200", "--json", "name"]));
const unlisted = existing.map((label) => label.name).filter((name) => !listed.has(name));

if (unlisted.length > 0) {
  process.stdout.write(`\nOn GitHub but not in ${LABELS_PATH}; remove by hand if no longer wanted:\n`);
  for (const name of unlisted) {
    process.stdout.write(`  ${name}\n`);
  }
}
