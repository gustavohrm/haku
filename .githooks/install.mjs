#!/usr/bin/env node
// Points git at the committed hooks.
//
// `core.hooksPath` is a local setting, so a clone runs no hooks until something
// sets it. Wiring it from `prepare` means an install is all it takes, with no
// hook manager to add as a dependency.
//
// Nothing here is fatal: a source tree without git, or without permission to
// write its config, must still install.
import { spawnSync } from "node:child_process";

const HOOKS_PATH = ".githooks";

const result = spawnSync("git", ["config", "core.hooksPath", HOOKS_PATH], {
  encoding: "utf8",
  shell: false,
  stdio: ["ignore", "pipe", "pipe"],
});

if (result.error !== undefined || result.status !== 0) {
  const reason = result.error?.message ?? result.stderr?.trim() ?? `git exited with ${String(result.status)}`;
  process.stdout.write(`Skipped git hook setup: ${reason}\n`);
}
