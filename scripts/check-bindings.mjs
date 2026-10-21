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
//
// The test is selected by name, so a rename would make the filter match nothing: cargo would exit 0
// and the check would pass having regenerated nothing. The run must therefore report exactly one test.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const BINDINGS_PATH = "src/bindings.ts";

const before = readFileSync(BINDINGS_PATH, "utf8");

const result = spawnSync(
  "cargo",
  ["test", "--manifest-path", "tauri/Cargo.toml", "--locked", "--lib", "ipc::tests::export_bindings", "--", "--exact"],
  { stdio: ["inherit", "pipe", "inherit"], encoding: "utf8", shell: false },
);

// Passed through so the test output stays visible, as it was when stdout was inherited.
const output = result.stdout ?? "";
process.stdout.write(output);

if (result.error !== undefined || result.status !== 0) {
  process.stderr.write(`\nbindings:check: could not regenerate ${BINDINGS_PATH}.\n`);
  process.exit(1);
}

if (!/^running 1 test$/m.test(output)) {
  process.stderr.write(
    `\nbindings:check: cargo did not run exactly one test, so ${BINDINGS_PATH} was not regenerated.\n` +
      "The test ipc::tests::export_bindings may have been renamed; update scripts/check-bindings.mjs.\n",
  );
  process.exit(1);
}

if (readFileSync(BINDINGS_PATH, "utf8") !== before) {
  process.stderr.write(
    `\nbindings:check: ${BINDINGS_PATH} was out of date with the Rust it is generated from.\n` +
      "It has been regenerated; review the change and commit it.\n",
  );
  process.exit(1);
}
