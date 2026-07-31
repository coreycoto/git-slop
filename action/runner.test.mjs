import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const actionDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(actionDirectory, "..");
const runner = join(actionDirectory, "runner.mjs");

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "git-slop-action-test-"));
  const repository = join(root, "repository");
  const output = join(root, "output.txt");
  const summary = join(root, "summary.md");
  const fakeBinary = join(root, "git-slop");
  execFileSync("git", ["init", repository], { stdio: "ignore" });
  writeFileSync(
    fakeBinary,
    `#!/usr/bin/env node
const fs = require("fs");
const path = require("path");
const args = process.argv.slice(2);
if (args[0] === "find") {
  const latest = path.join(process.cwd(), ".slop", "latest");
  fs.mkdirSync(latest, { recursive: true });
  const counter = path.join(process.cwd(), ".find-count");
  fs.writeFileSync(counter, String(Number(fs.existsSync(counter) ? fs.readFileSync(counter, "utf8") : "0") + 1));
  fs.writeFileSync(path.join(latest, "report.json"), JSON.stringify({ health: { findings: [{}, {}] } }));
  fs.writeFileSync(path.join(latest, "report.yaml"), "schema_version: 4\\n");
  fs.writeFileSync(path.join(latest, "summary.md"), "# Summary\\n");
  fs.writeFileSync(path.join(latest, "health.md"), "# Repository Health\\n\\nHealthy fixture.\\n");
  process.stdout.write("src/untrusted\\n::error title=forged::message\\n");
  process.exit(0);
}
if (args[0] === "health") {
  process.stdout.write("::warning file=src/example.rs,title=Git Slop::Fixture finding%0ANext: git-slop explain --path src/example.rs\\n");
  process.exit(0);
}
if (args[0] === "check") {
  process.exit(1);
}
process.exit(2);
`,
    { mode: 0o755 },
  );
  chmodSync(fakeBinary, 0o755);
  return { root, repository, output, summary, fakeBinary };
}

function run(command, environment) {
  return spawnSync(process.execPath, [runner, command], {
    encoding: "utf8",
    env: {
      ...process.env,
      ...environment,
    },
  });
}

function outputs(path) {
  const lines = readFileSync(path, "utf8").split(/\r?\n/u);
  const result = {};
  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(/^([^<]+)<<(.+)$/u);
    if (!match) {
      continue;
    }
    const [, name, delimiter] = match;
    const values = [];
    index += 1;
    while (index < lines.length && lines[index] !== delimiter) {
      values.push(lines[index]);
      index += 1;
    }
    result[name] = values.join("\n");
  }
  return result;
}

test("safe defaults analyze once and select only health.md", () => {
  const state = fixture();
  const analysis = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
  });
  assert.equal(analysis.status, 0, analysis.stderr);
  assert.doesNotMatch(analysis.stdout, /::error/u);
  assert.equal(readFileSync(join(state.repository, ".find-count"), "utf8"), "1");
  assert.match(readFileSync(state.summary, "utf8"), /# Repository Health/u);
  const outputs = readFileSync(state.output, "utf8");
  assert.match(outputs, /policy<<[\s\S]*\nadvisory\n/u);
  assert.match(outputs, /artifact-contents<<[\s\S]*\nsummary\n/u);
  assert.match(outputs, /retention-days<<[\s\S]*\n14\n/u);
  assert.match(outputs, /finding-count<<[\s\S]*\n2\n/u);

  writeFileSync(state.output, "");
  const artifacts = run("artifacts", {
    GITHUB_OUTPUT: state.output,
    GIT_SLOP_ARTIFACT_CONTENTS: "summary",
    GIT_SLOP_HEALTH_PATH: join(state.repository, ".slop", "latest", "health.md"),
    GIT_SLOP_REPORT_PATH: join(state.repository, ".slop", "latest", "report.json"),
    GIT_SLOP_REPORT_YAML_PATH: join(state.repository, ".slop", "latest", "report.yaml"),
    GIT_SLOP_SUMMARY_PATH: join(state.repository, ".slop", "latest", "summary.md"),
  });
  assert.equal(artifacts.status, 0, artifacts.stderr);
  const artifactOutput = readFileSync(state.output, "utf8");
  assert.match(artifactOutput, /health\.md/u);
  assert.doesNotMatch(artifactOutput, /report\.json/u);
});

test("annotations are bounded without rerunning analysis", () => {
  const state = fixture();
  const analysis = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
  });
  assert.equal(analysis.status, 0, analysis.stderr);
  writeFileSync(state.output, "");
  const annotation = run("annotate", {
    GITHUB_OUTPUT: state.output,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_REPORT_PATH: join(state.repository, ".slop", "latest", "report.json"),
    GIT_SLOP_FINDING_COUNT: "2",
    GIT_SLOP_MAX_ANNOTATIONS: "1",
    GIT_SLOP_WORKING_DIRECTORY_RESOLVED: state.repository,
  });
  assert.equal(annotation.status, 0, annotation.stderr);
  assert.match(annotation.stdout, /::warning/u);
  assert.match(readFileSync(state.output, "utf8"), /annotation-count<<[\s\S]*\n1\n/u);
  assert.equal(readFileSync(join(state.repository, ".find-count"), "utf8"), "1");
});

test("nested working-directory resolves outputs at the Git worktree root", () => {
  const state = fixture();
  const nested = join(state.repository, "packages", "application");
  mkdirSync(nested, { recursive: true });
  const analysis = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: "packages/application",
  });
  assert.equal(analysis.status, 0, analysis.stderr);
  const actual = outputs(state.output);
  const repositoryRoot = realpathSync(state.repository);
  assert.equal(actual["working-directory"], repositoryRoot);
  assert.equal(actual["report-path"], join(repositoryRoot, ".slop", "latest", "report.json"));
  assert.equal(readFileSync(join(state.repository, ".find-count"), "utf8"), "1");
});

test("analysis failure cannot publish stale report-sized artifacts", () => {
  const state = fixture();
  const successful = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
    GIT_SLOP_ARTIFACT_CONTENTS: "full",
  });
  assert.equal(successful.status, 0, successful.stderr);

  writeFileSync(state.output, "");
  const failed = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: join(state.root, "missing-git-slop"),
    GIT_SLOP_INSTALL_OUTCOME: "failure",
    GIT_SLOP_WORKING_DIRECTORY: ".",
    GIT_SLOP_ARTIFACT_CONTENTS: "full",
  });
  assert.equal(failed.status, 0, failed.stderr);
  const actual = outputs(state.output);
  assert.equal(actual["analysis-exit-code"], "2");
  assert.equal(actual["artifact-contents"], "summary");
  assert.equal(actual["report-path"], "");
  assert.equal(actual["report-yaml-path"], "");
  assert.equal(actual["summary-path"], "");
});

test("advisory passes and enforce preserves the policy result", () => {
  const state = fixture();
  const report = join(state.repository, ".slop", "latest", "report.json");
  const analysis = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
  });
  assert.equal(analysis.status, 0, analysis.stderr);

  writeFileSync(state.output, "");
  const advisory = run("finalize", {
    GITHUB_OUTPUT: state.output,
    GIT_SLOP_ANALYSIS_EXIT_CODE: "0",
    GIT_SLOP_POLICY: "advisory",
  });
  assert.equal(advisory.status, 0, advisory.stderr);
  assert.match(readFileSync(state.output, "utf8"), /status<<[\s\S]*\nadvisory\n/u);

  writeFileSync(state.output, "");
  const enforce = run("finalize", {
    GITHUB_OUTPUT: state.output,
    GIT_SLOP_ANALYSIS_EXIT_CODE: "0",
    GIT_SLOP_POLICY: "enforce",
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_REPORT_PATH: report,
    GIT_SLOP_WORKING_DIRECTORY_RESOLVED: state.repository,
  });
  assert.equal(enforce.status, 1);
  assert.match(readFileSync(state.output, "utf8"), /policy-exit-code<<[\s\S]*\n1\n/u);
  assert.equal(readFileSync(join(state.repository, ".find-count"), "utf8"), "1");
});

test("publication failure is reflected in final status", () => {
  const state = fixture();
  const finalized = run("finalize", {
    GITHUB_OUTPUT: state.output,
    GIT_SLOP_ANALYSIS_EXIT_CODE: "0",
    GIT_SLOP_POLICY: "advisory",
    GIT_SLOP_UPLOAD_OUTCOME: "failure",
  });
  assert.equal(finalized.status, 1);
  const actual = outputs(state.output);
  assert.equal(actual.status, "error");
  assert.equal(actual["policy-exit-code"], "2");
  assert.match(finalized.stderr, /publication failed: artifact upload/u);
});

test("metadata and installer pin bounded secure defaults and supported targets", () => {
  const metadata = readFileSync(join(repositoryRoot, "action.yml"), "utf8");
  const installer = readFileSync(join(actionDirectory, "install.mjs"), "utf8");
  assert.match(metadata, /default: advisory/u);
  assert.match(metadata, /default: summary/u);
  assert.match(metadata, /default: "14"/u);
  assert.match(metadata, /default: "false"/u);
  assert.match(installer, /SHA256SUMS/u);
  assert.match(installer, /createHash\("sha256"\)/u);
  assert.match(installer, /x86_64-unknown-linux-gnu/u);
  assert.match(installer, /aarch64-apple-darwin/u);
  assert.match(installer, /x86_64-pc-windows-msvc/u);
  assert.match(installer, /process\.platform === "win32" \? "zip" : "tar\.gz"/u);
  assert.match(installer, /output !== `git-slop \$\{version\}`/u);
});
