#!/usr/bin/env node
// Checks this repository against the template commit it last synced with, as `.template/README.md`
// describes: the manifest's shape, the files it lists, project-block markers, local edits to the
// template's text, and placeholders left behind. It changes nothing, and exits 1 on any problem.
//
// It fetches the template, so it needs the network. It adds no remote, ref, or FETCH_HEAD: the fetched
// objects sit unreferenced in the object store until git collects them.
//
// Usage: node .template/check.mjs
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const MANIFEST = ".template/manifest.json";
const CLASSES = new Set(["canonical", "blocks", "seed"]);
// An extended regular expression, for git grep.
const PLACEHOLDER = String.raw`\{\{(name|slug|scope|description|owner|holder|year)\}\}`;
// A marker is a line of its own: a comment opener, the marker, and for HTML the closer.
// A malformed marker, such as `<!-- project:start` without its closer, is not one: it stays template text, so
// the comparison reports it instead of it hiding the lines after it.
const MARKER = /^\s*(?:<!--\s*project:(start|end)\s*-->|(?:#|\/\/)\s*project:(start|end))\s*$/;

/**
 * Says which marker a line is, if any.
 * @param {string} line The line.
 * @returns {"start" | "end" | undefined} The marker's kind.
 */
function markerKind(line) {
  const match = MARKER.exec(line);
  return match === null ? undefined : /** @type {"start" | "end"} */ (match[1] ?? match[2]);
}
const SHA = /^[0-9a-f]{40}$/;
const REPOSITORY = /^[\w.-]+\/[\w.-]+$/;

/**
 * Runs git in `cwd` and returns its exit status and output.
 * @param {string} cwd The repository to run in.
 * @param {string[]} args The git arguments.
 * @returns {{ status: number | null, stdout: string, stderr: string }} The result.
 */
function git(cwd, args) {
  const result = spawnSync("git", args, { cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  if (result.error !== undefined) {
    throw result.error;
  }
  return { status: result.status, stdout: result.stdout, stderr: result.stderr.trim() };
}

/**
 * Splits text into lines, ignoring a CRLF checkout.
 * @param {string} text The file's text.
 * @returns {string[]} Its lines.
 */
function lines(text) {
  return text.replaceAll("\r\n", "\n").split("\n");
}

/**
 * Lists what is wrong with a file's project-block markers.
 * @param {string} text The file's text.
 * @returns {string[]} One message per problem, each naming its line.
 */
export function markerProblems(text) {
  const problems = [];
  let openedAt = 0;
  for (const [index, line] of lines(text).entries()) {
    const kind = markerKind(line);
    if (kind === "start") {
      if (openedAt !== 0) {
        problems.push(`line ${index + 1}: project:start inside the block opened at line ${openedAt}`);
      }
      openedAt = index + 1;
    } else if (kind === "end") {
      if (openedAt === 0) {
        problems.push(`line ${index + 1}: project:end without a project:start`);
      }
      openedAt = 0;
    }
  }
  if (openedAt !== 0) {
    problems.push(`line ${openedAt}: project:start is never closed`);
  }
  return problems;
}

/**
 * Keeps the template-owned lines of a `blocks` file: everything outside its project blocks, with the
 * markers themselves, and for a Markdown document its frontmatter keys without their values.
 * @param {string} text The file's text.
 * @param {boolean} isDocument Whether the file is Markdown, whose frontmatter values the project owns.
 * @returns {{ text: string, line: number }[]} The kept lines, with their 1-based line numbers.
 */
export function templateLines(text, isDocument) {
  const all = lines(text);
  const kept = [];
  let next = 0;
  if (isDocument && all[0] === "---") {
    const end = all.indexOf("---", 1);
    if (end > 0) {
      for (const [index, line] of all.slice(0, end + 1).entries()) {
        // Only the delimiters and the keys are the template's. Any other line, indented or blank, continues the
        // value above it, such as a folded `>` scalar, and is the project's too.
        if (line === "---" || /^[\w-]+:/.test(line)) {
          kept.push({ text: line.replace(/^([\w-]+):.*$/, "$1:"), line: index + 1 });
        }
      }
      next = end + 1;
    }
  }
  let isInBlock = false;
  for (const [offset, line] of all.slice(next).entries()) {
    const kind = markerKind(line);
    if (kind !== undefined) {
      isInBlock = kind === "start";
    }
    if (kind !== undefined || !isInBlock) {
      kept.push({ text: line, line: next + offset + 1 });
    }
  }
  return kept;
}

/**
 * Lists what is wrong with the manifest's shape.
 * @param {unknown} manifest The parsed manifest.
 * @returns {string[]} One message per problem.
 */
function shapeProblems(manifest) {
  if (typeof manifest !== "object" || manifest === null || Array.isArray(manifest)) {
    return ["is not a JSON object"];
  }
  const fields = /** @type {Record<string, unknown>} */ (manifest);
  const problems = [];
  if (typeof fields.template !== "string" || !REPOSITORY.test(fields.template)) {
    problems.push("template must be an owner/repo string");
  }
  if (fields.synced !== null && (typeof fields.synced !== "string" || !SHA.test(fields.synced))) {
    problems.push("synced must be null or a full 40-character commit SHA");
  }
  const files = fields.files;
  if (typeof files !== "object" || files === null || Array.isArray(files)) {
    problems.push("files must be an object of path to class");
    return problems;
  }
  for (const [path, fileClass] of Object.entries(files)) {
    if (!CLASSES.has(fileClass)) {
      problems.push(`files: ${path} has class ${JSON.stringify(fileClass)}, not canonical, blocks, or seed`);
    }
  }
  const exceptions = fields.exceptions ?? {};
  if (typeof exceptions !== "object" || exceptions === null || Array.isArray(exceptions)) {
    problems.push("exceptions must be an object of path to reason");
    return problems;
  }
  for (const [path, reason] of Object.entries(exceptions)) {
    if (typeof reason !== "string" || reason.trim() === "") {
      problems.push(`exceptions: ${path} needs a reason`);
    }
    if (!Object.hasOwn(files, path)) {
      problems.push(`exceptions: ${path} is not in files`);
    } else if (/** @type {Record<string, unknown>} */ (files)[path] === "seed") {
      problems.push(`exceptions: ${path} is a seed file, which the project already owns`);
    }
  }
  return problems;
}

/**
 * Checks a repository against the template commit its manifest records.
 * @param {{ cwd?: string, templateUrl?: string }} [options] The repository, and the template's URL when it
 *   is not `https://github.com/<template>`.
 * @returns {{ problems: string[], notes: string[] }} What fails the check, and what is only reported.
 */
export function check({ cwd = process.cwd(), templateUrl } = {}) {
  /** @type {string[]} */
  const problems = [];
  /** @type {string[]} */
  const notes = [];

  let manifest;
  try {
    manifest = JSON.parse(readFileSync(join(cwd, MANIFEST), "utf8"));
  } catch (error) {
    return { problems: [`${MANIFEST}: ${error instanceof Error ? error.message : String(error)}`], notes };
  }
  const shape = shapeProblems(manifest);
  if (shape.length > 0) {
    return { problems: shape.map((problem) => `${MANIFEST}: ${problem}`), notes };
  }

  /** @type {Record<string, string>} */
  const files = manifest.files;
  /** @type {Record<string, string>} */
  const exceptions = manifest.exceptions ?? {};
  const excused = new Set();
  /** Records a difference from the template, unless an exception excuses the file. */
  const report = (/** @type {string} */ path, /** @type {string} */ message) => {
    if (Object.hasOwn(exceptions, path)) {
      excused.add(path);
    } else {
      problems.push(message);
    }
  };

  const owned = Object.keys(files).filter((path) => files[path] !== "seed");
  const present = owned.filter((path) => {
    if (existsSync(join(cwd, path))) {
      return true;
    }
    report(path, `${path}: missing; the manifest lists it as ${files[path]}`);
    return false;
  });
  const blocks = present.filter((path) => files[path] === "blocks");
  for (const path of blocks) {
    for (const problem of markerProblems(readFileSync(join(cwd, path), "utf8"))) {
      // Unconditional: an exception excuses a difference from the template, not a broken file.
      problems.push(`${path}: ${problem}`);
    }
  }

  if (manifest.synced === null) {
    notes.push("synced is null, so there is no template commit to compare with.");
    return { problems, notes };
  }
  const synced = manifest.synced;
  const url = templateUrl ?? `https://github.com/${manifest.template}`;

  const head = git(cwd, ["ls-remote", url, "HEAD"]);
  const headSha = head.stdout.split(/\s/)[0] ?? "";
  if (head.status !== 0 || !SHA.test(headSha)) {
    problems.push(`could not read ${url}: ${head.stderr || "no HEAD"}`);
    return { problems, notes };
  }
  const fetched = git(cwd, ["fetch", "--quiet", "--no-tags", "--no-write-fetch-head", url, "HEAD"]);
  if (fetched.status !== 0) {
    problems.push(`could not fetch ${url}: ${fetched.stderr}`);
    return { problems, notes };
  }
  // `synced` counts only in the history of the template's default branch, which is where a sync takes it
  // from. A commit already in this repository proves nothing about the template, and fetching it by SHA
  // cannot prove more: git skips the fetch of an object it already has.
  const hasSynced = git(cwd, ["cat-file", "-e", `${synced}^{commit}`]).status === 0;
  const ancestry = hasSynced ? git(cwd, ["merge-base", "--is-ancestor", synced, headSha]) : { status: 1, stderr: "" };
  if (ancestry.status !== 0) {
    problems.push(
      ancestry.status === 1
        ? `${MANIFEST}: synced ${synced} is not in the history of ${url}'s default branch`
        : `could not compare synced ${synced} with ${url}: ${ancestry.stderr}`,
    );
    return { problems, notes };
  }
  const count = git(cwd, ["rev-list", "--count", `${synced}..${headSha}`]);
  if (count.status !== 0) {
    problems.push(`could not count the commits since synced ${synced}: ${count.stderr}`);
    return { problems, notes };
  }
  const behind = Number(count.stdout.trim());
  notes.push(
    behind === 0
      ? `Up to date with ${manifest.template} at ${headSha.slice(0, 7)}.`
      : `${behind} commit(s) behind ${manifest.template}: ${synced.slice(0, 7)}..${headSha.slice(0, 7)}.`,
  );

  const theirs = git(cwd, ["show", `${synced}:${MANIFEST}`]);
  /** @type {Record<string, string>} */
  let theirFiles = {};
  try {
    theirFiles = JSON.parse(theirs.stdout).files ?? {};
  } catch {
    problems.push(`${MANIFEST}: the template has no readable manifest at ${synced.slice(0, 7)}`);
  }
  for (const path of new Set([...Object.keys(files), ...Object.keys(theirFiles)])) {
    if (files[path] !== theirFiles[path]) {
      problems.push(
        `${MANIFEST}: files lists ${path} as ${files[path] ?? "nothing"}; the template at ${synced.slice(0, 7)} lists it as ${theirFiles[path] ?? "nothing"}`,
      );
    }
  }

  const canonical = present.filter((path) => files[path] === "canonical");
  if (canonical.length > 0) {
    const changed = git(cwd, [
      "-c",
      "core.quotePath=false",
      "diff",
      "--no-ext-diff",
      "--name-only",
      "-z",
      synced,
      "--",
      ...canonical,
    ]);
    for (const path of changed.stdout.split("\0").filter(Boolean)) {
      report(path, `${path}: canonical, but differs from the template at ${synced.slice(0, 7)}`);
    }
  }

  for (const path of blocks) {
    const original = git(cwd, ["show", `${synced}:${path}`]);
    if (original.status !== 0) {
      report(path, `${path}: not in the template at ${synced.slice(0, 7)}`);
      continue;
    }
    const isDocument = path.endsWith(".md");
    const ours = templateLines(readFileSync(join(cwd, path), "utf8"), isDocument);
    const expected = templateLines(original.stdout, isDocument);
    const index = ours.findIndex((line, at) => line.text !== expected[at]?.text);
    if (index !== -1 || ours.length !== expected.length) {
      const line = (ours[index] ?? ours.at(-1))?.line ?? 1;
      report(path, `${path}:${line}: differs from the template's text outside the project blocks`);
    }
  }

  for (const path of Object.keys(exceptions)) {
    if (!excused.has(path)) {
      problems.push(`${MANIFEST}: exceptions lists ${path}, which matches the template; remove the exception`);
    }
  }

  const placeholderFiles = Object.keys(files).filter(
    (path) => files[path] !== "canonical" && existsSync(join(cwd, path)),
  );
  if (placeholderFiles.length > 0) {
    // Literal pathspecs, so a listed path is never read as pathspec magic, and NUL separators, so a path with a
    // colon in it parses. git grep exits 1 when nothing matches, and higher on an error.
    const found = git(cwd, [
      "--literal-pathspecs",
      "grep",
      "--no-index",
      "-z",
      "-nIE",
      PLACEHOLDER,
      "--",
      ...placeholderFiles,
    ]);
    if (found.status !== 0 && found.status !== 1) {
      problems.push(`could not scan for placeholders: ${found.stderr}`);
    }
    for (const match of found.stdout.split("\n").filter(Boolean)) {
      const [path, line] = match.split("\0");
      problems.push(`${path}:${line}: placeholder left behind`);
    }
  }

  if (Object.hasOwn(files, ".githooks/install.mjs")) {
    const hooksPath = git(cwd, ["config", "--get", "core.hooksPath"]).stdout.trim();
    if (hooksPath !== ".githooks") {
      problems.push(`core.hooksPath is ${hooksPath === "" ? "unset" : hooksPath}; pnpm install sets it to .githooks`);
    }
  }

  return { problems, notes };
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { problems, notes } = check();
  for (const note of notes) {
    process.stdout.write(`${note}\n`);
  }
  for (const problem of problems) {
    process.stdout.write(`problem: ${problem}\n`);
  }
  if (problems.length > 0) {
    process.stdout.write(
      `${problems.length} problem(s). .template/README.md says how to resolve each: sync, propose the change to the template, or list a deliberate difference in exceptions.\n`,
    );
    process.exitCode = 1;
  }
}
