#!/usr/bin/env node
// Fails when `src/bindings.ts` no longer matches the Rust it is generated from.
//
// The bindings are written by a cargo test, so the only way to know whether the
// committed file is current is to regenerate it and compare. Comparing against
// the file as it was before the run, rather than against git, is what makes
// this work on a working tree with uncommitted Rust changes: a stale file fails
// whether or not anything is committed yet.
//
// A stale file is left regenerated, which is the content to review and commit.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const BINDINGS_PATH = "src/bindings.ts";

const before = readFileSync(BINDINGS_PATH, "utf8");

const result = spawnSync(
  "cargo",
  ["test", "--manifest-path", "tauri/Cargo.toml", "--locked", "--lib", "ipc::tests::export_bindings", "--", "--exact"],
  { stdio: "inherit", shell: false },
);

if (result.error !== undefined || result.status !== 0) {
  process.stderr.write(`\nbindings:check: could not regenerate ${BINDINGS_PATH}.\n`);
  process.exit(1);
}

if (readFileSync(BINDINGS_PATH, "utf8") !== before) {
  process.stderr.write(
    `\nbindings:check: ${BINDINGS_PATH} was out of date with the Rust it is generated from.\n` +
      "It has been regenerated; review the change and commit it.\n",
  );
  process.exit(1);
}
