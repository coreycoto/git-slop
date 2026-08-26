import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { run } from "./runtime.mjs";
import { buildSurfaceAreaMarkdown } from "./surface-area.mjs";

function git(cwd, ...args) {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
}

test("surface ledger reports multidimensional pull-request deltas without a score", () => {
  const repository = mkdtempSync(join(tmpdir(), "git-slop-surface-test-"));
  git(repository, "init", "--quiet");
  git(repository, "config", "user.name", "git-slop");
  git(repository, "config", "user.email", "git-slop@users.noreply.github.com");
  writeFileSync(join(repository, "README.md"), "# Fixture\n");
  git(repository, "add", "README.md");
  git(repository, "commit", "--quiet", "-m", "base");
  const base = git(repository, "rev-parse", "HEAD");
  const eventPath = join(repository, "event.json");
  writeFileSync(eventPath, JSON.stringify({ pull_request: { base: { sha: base } } }));

  writeFileSync(join(repository, "SKILL.md"), "---\nname: fixture\ndescription: fixture\n---\n");
  writeFileSync(join(repository, "README.md"), "# Fixture\n\nMore docs.\n");
  git(repository, "add", "README.md", "SKILL.md");

  const markdown = buildSurfaceAreaMarkdown(run, repository, { eventPath });
  assert.match(markdown, /## Surface-area ledger/u);
  assert.match(markdown, /\| Tracked files \| 1 \| 2 \| \+1 \|/u);
  assert.match(markdown, /\| Agent skill definitions \| 0 \| 1 \| \+1 \|/u);
  assert.match(markdown, /1 added, 0 removed, 1 retained path\(s\) changed/u);
  assert.match(markdown, /there is no composite score/u);
  assert.match(markdown, /reassess after 10 pull requests/u);
  assert.doesNotMatch(markdown, /surface[_ -]score/iu);
});

test("surface ledger reports head dimensions when no pull-request base is available", () => {
  const repository = mkdtempSync(join(tmpdir(), "git-slop-surface-head-test-"));
  git(repository, "init", "--quiet");
  writeFileSync(join(repository, "README.md"), "# Fixture\n");
  git(repository, "add", "README.md");

  const markdown = buildSurfaceAreaMarkdown(run, repository, { scope: "." });
  assert.match(markdown, /\| Tracked files \| 1 \|/u);
  assert.match(markdown, /no pull-request base revision/u);
  assert.match(markdown, /\*\*Scope:\*\* whole repository/u);
});
