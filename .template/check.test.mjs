import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { check, markerProblems, templateLines } from "./check.mjs";

// Placeholders are built, not written, so this file holds none for the check to find in a project.
const placeholder = (/** @type {string} */ name) => `{${"{"}${name}}}`;

const TEMPLATE_FILES = {
  ".editorconfig": "root = true\n",
  "AGENTS.md": [
    "---",
    "status: DRAFT",
    "---",
    "",
    "# Agents",
    "",
    "<!-- project:start -->",
    "",
    "<!-- project:end -->",
    "",
    "Template text.",
    "",
  ].join("\n"),
  "README.md": `# ${placeholder("name")}\n`,
};
const CLASSES = { ".editorconfig": "canonical", "AGENTS.md": "blocks", "README.md": "seed" };

/** @type {string} */
let root;

/**
 * Runs git and returns its trimmed output, throwing when it fails.
 * @param {string} cwd The repository to run in.
 * @param {...string} args The git arguments.
 * @returns {string} Its output.
 */
function git(cwd, ...args) {
  const result = spawnSync("git", args, { cwd, encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(`git ${args.join(" ")}: ${result.stderr}`);
  }
  return result.stdout.trim();
}

/**
 * Writes files, creating their directories.
 * @param {string} cwd The repository to write in.
 * @param {Record<string, string>} files Each path and its text.
 */
function write(cwd, files) {
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(cwd, path)), { recursive: true });
    writeFileSync(join(cwd, path), text);
  }
}

/**
 * Writes files and commits everything.
 * @param {string} cwd The repository to commit in.
 * @param {Record<string, string>} files Each path and its text.
 * @returns {string} The new commit's SHA.
 */
function commit(cwd, files) {
  write(cwd, files);
  git(cwd, "add", "-A");
  git(cwd, "commit", "--quiet", "-m", "change");
  return git(cwd, "rev-parse", "HEAD");
}

/**
 * A manifest's text, with the test classes unless `fields` replaces them.
 * @param {Record<string, unknown>} fields Fields to set over the defaults.
 * @returns {string} The JSON text.
 */
function manifest(fields) {
  return `${JSON.stringify({ template: "acme/template", synced: null, files: CLASSES, ...fields }, null, 2)}\n`;
}

/** @type {string} */
let fixture;
/** @type {string} */
let fixtureSynced;

/**
 * A template repository and a project created from its first commit. Both are built once, in `beforeAll`, and
 * copied for each test: building them takes eight git processes, about a second per test on Windows.
 */
function setUp() {
  const template = join(root, "template");
  const project = join(root, "project");
  cpSync(fixture, root, { recursive: true });
  return {
    template,
    project,
    synced: fixtureSynced,
    run: () => check({ cwd: project, templateUrl: template }),
  };
}

beforeAll(() => {
  fixture = mkdtempSync(join(tmpdir(), "template-check-fixture-"));
  const config = join(fixture, "gitconfig");
  writeFileSync(config, "[user]\n\tname = Test\n\temail = test@example.com\n[commit]\n\tgpgsign = false\n");
  vi.stubEnv("GIT_CONFIG_GLOBAL", config);
  vi.stubEnv("GIT_CONFIG_NOSYSTEM", "1");
  const template = join(fixture, "template");
  const project = join(fixture, "project");
  mkdirSync(template);
  mkdirSync(project);
  git(template, "init", "--quiet", "-b", "main");
  fixtureSynced = commit(template, { ...TEMPLATE_FILES, ".template/manifest.json": manifest({}) });
  git(project, "init", "--quiet", "-b", "main");
  commit(project, {
    ...TEMPLATE_FILES,
    "README.md": "# Acme Notes\n",
    ".template/manifest.json": manifest({ synced: fixtureSynced }),
  });
});

afterAll(() => {
  vi.unstubAllEnvs();
  rmSync(fixture, { recursive: true, force: true });
});

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "template-check-"));
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

// Each test runs git a dozen times or more, which takes seconds on Windows under load.
describe("check", { timeout: 60_000 }, () => {
  it("passes a project that matches the template and reports it up to date", () => {
    const { run } = setUp();

    expect(run()).toEqual({ problems: [], notes: [expect.stringMatching(/^Up to date with acme\/template/)] });
  });

  it("reports how far behind the template the project is", () => {
    const { template, run } = setUp();
    commit(template, { "other.txt": "1\n" });
    commit(template, { "other.txt": "2\n" });

    expect(run().notes).toEqual([expect.stringMatching(/^2 commit\(s\) behind acme\/template/)]);
  });

  it("reports a local edit to a canonical file", () => {
    const { project, run } = setUp();
    write(project, { ".editorconfig": "root = false\n" });

    expect(run().problems).toEqual([expect.stringMatching(/^\.editorconfig: canonical, but differs/)]);
  });

  it("accepts project text inside a block and the project's own frontmatter values", () => {
    const { project, run } = setUp();
    write(project, {
      "AGENTS.md": TEMPLATE_FILES["AGENTS.md"]
        .replace("status: DRAFT", "status: APPROVED")
        .replace("<!-- project:start -->\n", "<!-- project:start -->\n\nAcme's own rules.\n"),
    });

    expect(run().problems).toEqual([]);
  });

  it("accepts a frontmatter value the project writes over several lines", () => {
    const { project, run } = setUp();
    write(project, {
      "AGENTS.md": TEMPLATE_FILES["AGENTS.md"].replace("status: DRAFT", "status: >\n  APPROVED\n\n  for Acme"),
    });

    expect(run().problems).toEqual([]);
  });

  it("reports an edit to a blocks file outside its blocks, with the line", () => {
    const { project, run } = setUp();
    write(project, { "AGENTS.md": TEMPLATE_FILES["AGENTS.md"].replace("Template text.", "Edited text.") });

    expect(run().problems).toEqual(["AGENTS.md:11: differs from the template's text outside the project blocks"]);
  });

  it("reports a removed block", () => {
    const { project, run } = setUp();
    write(project, {
      "AGENTS.md": TEMPLATE_FILES["AGENTS.md"].replace("<!-- project:start -->\n\n<!-- project:end -->\n", ""),
    });

    expect(run().problems).toEqual([expect.stringMatching(/^AGENTS\.md:\d+: differs/)]);
  });

  it("reports a missing canonical file, and accepts it when an exception gives the reason", () => {
    const { project, synced, run } = setUp();
    unlinkSync(join(project, ".editorconfig"));

    expect(run().problems).toEqual([".editorconfig: missing; the manifest lists it as canonical"]);

    write(project, {
      ".template/manifest.json": manifest({ synced, exceptions: { ".editorconfig": "Acme uses its own." } }),
    });

    expect(run().problems).toEqual([]);
  });

  it("reports an exception the project no longer needs", () => {
    const { project, synced, run } = setUp();
    write(project, { ".template/manifest.json": manifest({ synced, exceptions: { ".editorconfig": "Old." } }) });

    expect(run().problems).toEqual([expect.stringMatching(/exceptions lists \.editorconfig, which matches/)]);
  });

  it("reports a placeholder left in a seed file", () => {
    const { project, run } = setUp();
    write(project, { "README.md": `# ${placeholder("name")}\n` });

    expect(run().problems).toEqual(["README.md:1: placeholder left behind"]);
  });

  it("reports a files map that differs from the template's at the synced commit", () => {
    const { project, synced, run } = setUp();
    write(project, {
      ".template/manifest.json": manifest({ synced, files: { ...CLASSES, "README.md": "canonical" } }),
    });

    expect(run().problems).toContain(
      ".template/manifest.json: files lists README.md as canonical; the template at " +
        `${synced.slice(0, 7)} lists it as seed`,
    );
  });

  it("reports a synced commit the template does not have", () => {
    const { project, run } = setUp();
    write(project, { ".template/manifest.json": manifest({ synced: "0".repeat(40) }) });

    expect(run().problems).toEqual([expect.stringMatching(/synced 0{40} is not in the history of/)]);
  });

  it("reports a synced commit that exists only in the project", () => {
    const { project, run } = setUp();
    const local = git(project, "rev-parse", "HEAD");
    write(project, { ".template/manifest.json": manifest({ synced: local }) });

    expect(run().problems).toEqual([expect.stringMatching(new RegExp(`synced ${local} is not in the history of`))]);
  });

  it("reports a placeholder in a file not yet committed", () => {
    const { project, run } = setUp();
    git(project, "rm", "--quiet", "--cached", "README.md");
    write(project, { "README.md": `# ${placeholder("name")}\n` });

    expect(run().problems).toEqual(["README.md:1: placeholder left behind"]);
  });

  it("reports a synced commit the template has only on another branch", () => {
    const { template, project, run } = setUp();
    git(template, "switch", "--quiet", "-c", "side");
    const side = commit(template, { "side.txt": "1\n" });
    git(template, "switch", "--quiet", "main");
    git(project, "fetch", "--quiet", template, "side");
    write(project, { ".template/manifest.json": manifest({ synced: side }) });

    expect(run().problems).toEqual([expect.stringMatching(new RegExp(`synced ${side} is not in the history of`))]);
  });

  it("reports a placeholder in a file git ignores", () => {
    const { project, run } = setUp();
    git(project, "rm", "--quiet", "--cached", "README.md");
    write(project, { ".gitignore": "README.md\n", "README.md": `# ${placeholder("name")}\n` });

    expect(run().problems).toEqual(["README.md:1: placeholder left behind"]);
  });

  it("reports unbalanced markers even in a file listed in exceptions", () => {
    const { project, synced, run } = setUp();
    write(project, {
      ".template/manifest.json": manifest({ synced, exceptions: { "AGENTS.md": "Acme rewrote it." } }),
      "AGENTS.md": "# Acme\n\n<!-- project:start -->\n",
    });

    expect(run().problems).toEqual(["AGENTS.md: line 3: project:start is never closed"]);
  });

  it("reports a malformed marker as a difference instead of hiding the text after it", () => {
    const { project, run } = setUp();
    write(project, {
      "AGENTS.md": TEMPLATE_FILES["AGENTS.md"]
        .replace("<!-- project:start -->", "<!-- project:start")
        .replace("<!-- project:end -->", "<!-- project:end"),
    });

    expect(run().problems).toEqual(["AGENTS.md:7: differs from the template's text outside the project blocks"]);
  });

  it("reports a template it cannot reach", () => {
    const { project } = setUp();

    expect(check({ cwd: project, templateUrl: join(root, "nowhere") }).problems).toEqual([
      expect.stringMatching(/^could not read /),
    ]);
  });

  it("checks only the structure when synced is null, as in the template itself", () => {
    const { template } = setUp();

    expect(check({ cwd: template, templateUrl: join(root, "nowhere") })).toEqual({
      problems: [],
      notes: ["synced is null, so there is no template commit to compare with."],
    });
  });

  it("reports a manifest with the wrong shape", () => {
    const { project, run } = setUp();
    write(project, { ".template/manifest.json": manifest({ synced: "abc", files: { "a.txt": "owned" } }) });

    expect(run().problems).toEqual([
      ".template/manifest.json: synced must be null or a full 40-character commit SHA",
      '.template/manifest.json: files: a.txt has class "owned", not canonical, blocks, or seed',
    ]);
  });
});

describe("markerProblems", () => {
  it("names the line of an unclosed, nested, or stray marker", () => {
    expect(markerProblems("# project:start\n# project:start\n")).toEqual([
      "line 2: project:start inside the block opened at line 1",
      "line 2: project:start is never closed",
    ]);
    expect(markerProblems("// project:end\n")).toEqual(["line 1: project:end without a project:start"]);
  });

  it("ignores a marker quoted in prose", () => {
    expect(markerProblems("Open a block with `<!-- project:start -->`.\n")).toEqual([]);
    expect(markerProblems("<!-- project:start\n# project:end -->\n")).toEqual([]);
  });
});

describe("templateLines", () => {
  it("drops block contents and frontmatter values, and keeps line numbers", () => {
    const text = "---\nstatus: X\n---\na\n  # project:start\n  b\n  # project:end\nc";

    expect(templateLines(text, true)).toEqual([
      { text: "---", line: 1 },
      { text: "status:", line: 2 },
      { text: "---", line: 3 },
      { text: "a", line: 4 },
      { text: "  # project:start", line: 5 },
      { text: "  # project:end", line: 7 },
      { text: "c", line: 8 },
    ]);
  });
});
