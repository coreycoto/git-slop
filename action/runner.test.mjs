import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { isolatedActionEnvironment } from "./test-environment.mjs";

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
  fs.writeFileSync(
    path.join(latest, "report.json"),
    JSON.stringify({
      files: [
        { path: "src/one.rs", context_band: "critical", slop_band: "low" },
        { path: "src/two.rs", context_band: "compact", slop_band: "critical" },
      ],
      health: { findings: [{}, {}, {}, {}] },
    }),
  );
  fs.writeFileSync(path.join(latest, "report.yaml"), "schema_version: 4\\n");
  fs.writeFileSync(path.join(latest, "summary.md"), "# Summary\\n");
  fs.writeFileSync(path.join(latest, "health.md"), "# Repository Health\\n\\nHealthy fixture.\\n");
  const compressionIndex = args.indexOf("--compression");
  const compression = compressionIndex === -1 ? "none" : args[compressionIndex + 1];
  if (compression === "gzip") fs.writeFileSync(path.join(latest, "report.json.gz"), "gzip-fixture");
  if (compression === "zstd") fs.writeFileSync(path.join(latest, "report.json.zst"), "zstd-fixture");
  process.stdout.write("src/untrusted\\n::error title=forged::message\\n");
  process.exit(0);
}
if (args[0] === "report" && args[1] === "validate") {
  process.exit(process.env.FAKE_VALIDATE_FAIL === "true" ? 2 : 0);
}
if (args[0] === "html") {
  const outputIndex = args.indexOf("--output");
  const output = args[outputIndex + 1];
  fs.writeFileSync(output, "<!doctype html><title>Git Slop fixture</title>\\n");
  process.exit(0);
}
if (args[0] === "health") {
  const counter = path.join(process.cwd(), ".health-count");
  fs.writeFileSync(counter, String(Number(fs.existsSync(counter) ? fs.readFileSync(counter, "utf8") : "0") + 1));
  const annotations = [
    "::notice file=src/notice.rs,title=Git Slop::Notice finding%0ANext: git-slop explain --path src/notice.rs\\n",
    "::warning file=src/warning.rs,title=Git Slop::Warning finding%0ANext: git-slop explain --path src/warning.rs\\n",
    "::error file=src/error.rs,title=Git Slop::Error finding%0ANext: git-slop explain --path src/error.rs\\n",
    "::warning file=src/omitted.rs,title=Git Slop::Omitted finding%0ANext: git-slop explain --path src/omitted.rs\\n",
  ];
  const limitIndex = args.indexOf("--max-annotations");
  const maximum = limitIndex === -1 ? annotations.length : Number(args[limitIndex + 1]);
  process.stdout.write(annotations.slice(0, maximum).join(""));
  process.exit(0);
}
if (args[0] === "check") {
  const counter = path.join(process.cwd(), ".check-count");
  fs.writeFileSync(counter, String(Number(fs.existsSync(counter) ? fs.readFileSync(counter, "utf8") : "0") + 1));
  process.stdout.write(JSON.stringify({ schema_version: 1, command: "check", finding_count: 2, passed: false }));
  process.exit(args.includes("--evaluate-only") ? 0 : 1);
}
if (args[0] === "compare") {
  if (process.env.FAKE_COMPARE_FAIL === "true") {
    process.stderr.write("incompatible baseline\\n");
    process.exit(2);
  }
  process.stdout.write(JSON.stringify({
    schema_version: 1,
    command: "compare",
    base_report: { generated_at: new Date().toISOString(), analyzed_revision_at: process.env.FAKE_ANALYZED_REVISION_AT || new Date().toISOString() },
    head_report: { generated_at: new Date().toISOString(), analyzed_revision_at: new Date().toISOString() },
    summary: { regression_count: 1 },
    regressions: [{ path: "src/new.rs", status: "new", severity: "error", base_slop_score: null, head_slop_score: 90 }],
    baseline_compatible: true,
    compatibility_mismatches: [],
  }));
  process.exit(0);
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
    env: isolatedActionEnvironment(environment),
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
  const actual = outputs(state.output);
  assert.equal(actual.policy, "advisory");
  assert.equal(actual.mode, "advanced");
  assert.equal(actual["artifact-contents"], "summary");
  assert.equal(actual["retention-days"], "14");
  assert.equal(actual["health-finding-count"], "4");
  assert.equal(actual["policy-finding-count"], "2");
  assert.equal(actual["finding-count"], "2");
  assert.equal(readFileSync(join(state.repository, ".check-count"), "utf8"), "1");

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

test("full artifacts include a newly generated portable HTML report", () => {
  const state = fixture();
  const analysis = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
    GIT_SLOP_ARTIFACT_CONTENTS: "full",
  });
  assert.equal(analysis.status, 0, analysis.stderr);
  const actual = outputs(state.output);
  assert.equal(actual["artifact-contents"], "full");
  assert.ok(actual["html-path"].endsWith("report.html"));
  assert.match(readFileSync(actual["html-path"], "utf8"), /Git Slop fixture/u);

  writeFileSync(state.output, "");
  const artifacts = run("artifacts", {
    GITHUB_OUTPUT: state.output,
    GIT_SLOP_ARTIFACT_CONTENTS: "full",
    GIT_SLOP_HEALTH_PATH: actual["health-path"],
    GIT_SLOP_REPORT_PATH: actual["report-path"],
    GIT_SLOP_REPORT_YAML_PATH: actual["report-yaml-path"],
    GIT_SLOP_SUMMARY_PATH: actual["summary-path"],
    GIT_SLOP_HTML_PATH: actual["html-path"],
  });
  assert.equal(artifacts.status, 0, artifacts.stderr);
  const selected = readFileSync(state.output, "utf8");
  assert.match(selected, /report\.html/u);
  assert.match(selected, /report\.json/u);
  assert.match(selected, /summary\.md/u);
});

test("mode presets derive policy and enforcement without changing advanced inputs", () => {
  for (const [mode, expectedPolicy, expectedEnforcement] of [
    ["advisory", "advisory", "absolute"],
    ["absolute", "enforce", "absolute"],
  ]) {
    const state = fixture();
    const analysis = run("analyze", {
      GITHUB_OUTPUT: state.output,
      GITHUB_STEP_SUMMARY: state.summary,
      GITHUB_WORKSPACE: state.repository,
      GIT_SLOP_BINARY: state.fakeBinary,
      GIT_SLOP_WORKING_DIRECTORY: ".",
      GIT_SLOP_MODE: mode,
      GIT_SLOP_POLICY: "advisory",
      GIT_SLOP_ENFORCEMENT: "regression",
    });
    assert.equal(analysis.status, 0, analysis.stderr);
    assert.match(
      analysis.stdout,
      /::warning title=Git Slop Action deprecation::Outputs finding-count/u,
    );
    const actual = outputs(state.output);
    assert.equal(actual.mode, mode);
    assert.equal(actual.policy, expectedPolicy);
    assert.equal(actual.enforcement, expectedEnforcement);
  }

  const state = fixture();
  const baseline = join(state.repository, "baseline.json");
  writeFileSync(baseline, JSON.stringify({ schema_version: 5 }), "utf8");
  const regression = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
    GIT_SLOP_MODE: "regression",
    GIT_SLOP_BASELINE_REPORT: baseline,
    GIT_SLOP_REQUIRE_BASELINE_ANCESTOR: "false",
  });
  assert.equal(regression.status, 0, regression.stderr);
  const actual = outputs(state.output);
  assert.equal(actual.mode, "regression");
  assert.equal(actual.policy, "enforce");
  assert.equal(actual.enforcement, "regression");
});

test("post-find validation failure preserves fresh diagnostics and report outputs", () => {
  const state = fixture();
  const analysis = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
    GIT_SLOP_COMPRESSION: "gzip",
    FAKE_VALIDATE_FAIL: "true",
  });
  assert.equal(analysis.status, 0, analysis.stderr);
  const actual = outputs(state.output);
  assert.equal(actual["analysis-exit-code"], "2");
  assert.equal(actual["artifact-contents"], "report");
  assert.ok(actual["report-path"].endsWith("report.json"));
  assert.ok(actual["compressed-report-path"].endsWith("report.json.gz"));
  assert.ok(actual["analysis-error-path"].endsWith("analysis-error.md"));
  assert.match(readFileSync(actual["analysis-error-path"], "utf8"), /failed immediate validation/u);
});

test("bounded annotations preserve notice, warning, and error without rerunning analysis", () => {
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
    GIT_SLOP_MAX_ANNOTATIONS: "3",
    GIT_SLOP_WORKING_DIRECTORY_RESOLVED: state.repository,
  });
  assert.equal(annotation.status, 0, annotation.stderr);
  const levels = [...annotation.stdout.matchAll(/^::(notice|warning|error) /gmu)].map(
    (match) => match[1],
  );
  assert.deepEqual(levels, ["notice", "warning", "error"]);
  assert.match(annotation.stdout, /file=src\/notice\.rs/u);
  assert.match(annotation.stdout, /file=src\/warning\.rs/u);
  assert.match(annotation.stdout, /file=src\/error\.rs/u);
  assert.doesNotMatch(annotation.stdout, /src\/omitted\.rs/u);
  assert.match(readFileSync(state.output, "utf8"), /annotation-count<<[\s\S]*\n3\n/u);
  assert.equal(readFileSync(join(state.repository, ".health-count"), "utf8"), "1");
  assert.equal(readFileSync(join(state.repository, ".find-count"), "utf8"), "1");
});

test("baseline enforcement uses the native comparator and fails only on regressions", () => {
  const state = fixture();
  const baseline = join(state.repository, "baseline.json");
  writeFileSync(baseline, JSON.stringify({ schema_version: 5 }), "utf8");
  const analysis = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
    GIT_SLOP_BASELINE_REPORT: baseline,
    GIT_SLOP_ENFORCEMENT: "regression",
    GIT_SLOP_REQUIRE_BASELINE_ANCESTOR: "false",
  });
  assert.equal(analysis.status, 0, analysis.stderr);
  const values = outputs(state.output);
  assert.equal(values["regression-count"], "1");
  assert.equal(values["baseline-compatible"], "true");
  assert.equal(values["baseline-status"], "compatible");
  assert.equal(values["finding-count"], "1");

  writeFileSync(state.output, "");
  const finalized = run("finalize", {
    GITHUB_OUTPUT: state.output,
    GIT_SLOP_ANALYSIS_EXIT_CODE: "0",
    GIT_SLOP_POLICY: "enforce",
    GIT_SLOP_ENFORCEMENT: "regression",
    GIT_SLOP_REGRESSION_COUNT: "1",
  });
  assert.equal(finalized.status, 1);
  assert.match(finalized.stderr, /1 repository-health regression/u);
});

test("freshly generated baseline over an old revision is fresh and reports revision age separately", () => {
  const state = fixture();
  const baseline = join(state.repository, "baseline.json");
  writeFileSync(baseline, JSON.stringify({ schema_version: 5 }), "utf8");
  const analysis = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
    GIT_SLOP_BASELINE_REPORT: baseline,
    GIT_SLOP_MAX_BASELINE_AGE_DAYS: "1",
    FAKE_ANALYZED_REVISION_AT: "2000-01-01T00:00:00Z",
  });
  assert.equal(analysis.status, 0, analysis.stderr);
  const actual = outputs(state.output);
  assert.equal(actual["baseline-status"], "compatible");
  const comparison = JSON.parse(readFileSync(actual["comparison-path"], "utf8"));
  assert.ok(comparison.baseline_revision.revision_age_days > 9000);
});

test("baseline comparison errors preserve successful head artifacts", () => {
  const state = fixture();
  const baseline = join(state.repository, "baseline.json");
  writeFileSync(baseline, JSON.stringify({ schema_version: 5 }), "utf8");
  const analysis = run("analyze", {
    GITHUB_OUTPUT: state.output,
    GITHUB_STEP_SUMMARY: state.summary,
    GITHUB_WORKSPACE: state.repository,
    GIT_SLOP_BINARY: state.fakeBinary,
    GIT_SLOP_WORKING_DIRECTORY: ".",
    GIT_SLOP_BASELINE_REPORT: baseline,
    FAKE_COMPARE_FAIL: "true",
  });
  assert.equal(analysis.status, 0, analysis.stderr);
  const actual = outputs(state.output);
  assert.equal(actual["analysis-exit-code"], "0");
  assert.equal(actual["baseline-status"], "error");
  assert.equal(actual["comparison-path"], "");
  assert.ok(actual["report-path"].endsWith("report.json"));
  assert.ok(actual["health-path"].endsWith("health.md"));
  assert.ok(actual["comparison-error-path"].endsWith("comparison-error.md"));
  assert.match(readFileSync(actual["health-path"], "utf8"), /Healthy fixture/u);
  assert.match(readFileSync(actual["comparison-error-path"], "utf8"), /head analysis completed/u);
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
  assert.equal(actual["html-path"], "");
});

test("advisory passes and enforce preserves the policy result", () => {
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
    GIT_SLOP_ABSOLUTE_FINDING_COUNT: "2",
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
  const installer = [
    join(actionDirectory, "install.mjs"),
    ...readdirSync(join(actionDirectory, "install"), { withFileTypes: true })
      .filter((entry) => entry.isFile() && entry.name.endsWith(".mjs"))
      .map((entry) => join(actionDirectory, "install", entry.name))
      .sort(),
  ]
    .map((path) => readFileSync(path, "utf8"))
    .join("\n");
  assert.match(metadata, /default: advisory/u);
  assert.match(metadata, /default: summary/u);
  assert.match(metadata, /default: "14"/u);
  assert.match(metadata, /default: "false"/u);
  assert.match(installer, /SHA256SUMS/u);
  assert.match(installer, /createHash\("sha256"\)/u);
  assert.match(installer, /x86_64-unknown-linux-gnu/u);
  assert.match(installer, /aarch64-apple-darwin/u);
  assert.match(installer, /"win32:x64": "x86_64-pc-windows-msvc"/u);
  assert.match(installer, /"win32:arm64": "aarch64-pc-windows-msvc"/u);
  assert.match(installer, /"darwin:x64": "x86_64-apple-darwin"/u);
  assert.match(installer, /x86_64-unknown-linux-musl/u);
  assert.match(installer, /process\.platform === "win32" \? "zip" : "tar\.gz"/u);
  assert.match(installer, /output !== `git-slop \$\{version\}`/u);
});
