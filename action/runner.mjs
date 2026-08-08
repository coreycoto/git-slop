import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

function appendFileCommand(target, name, value) {
  if (!target) {
    return;
  }
  const delimiter = `git_slop_${name}_${Date.now()}_${Math.random().toString(16).slice(2)}`;
  writeFileSync(target, `${name}<<${delimiter}\n${value}\n${delimiter}\n`, { flag: "a" });
}

function setOutput(name, value) {
  appendFileCommand(process.env.GITHUB_OUTPUT, name, String(value));
}

function normalizedBoolean(name, fallback) {
  const raw = (process.env[name] || fallback).trim().toLowerCase();
  if (raw !== "true" && raw !== "false") {
    throw new Error(`${name} must be true or false, received ${JSON.stringify(raw)}`);
  }
  return raw === "true";
}

function boundedInteger(name, fallback, minimum, maximum) {
  const raw = (process.env[name] || String(fallback)).trim();
  if (!/^\d+$/u.test(raw)) {
    throw new Error(`${name} must be an integer, received ${JSON.stringify(raw)}`);
  }
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`${name} must be from ${minimum} through ${maximum}, received ${raw}`);
  }
  return value;
}

function enumValue(name, fallback, allowed) {
  const raw = (process.env[name] || fallback).trim().toLowerCase();
  if (!allowed.includes(raw)) {
    throw new Error(`${name} must be one of ${allowed.join(", ")}, received ${JSON.stringify(raw)}`);
  }
  return raw;
}

function optionalBand(name, allowed) {
  const raw = (process.env[name] || "").trim().toLowerCase();
  if (raw && !allowed.includes(raw)) {
    throw new Error(`${name} must be empty or one of ${allowed.join(", ")}, received ${JSON.stringify(raw)}`);
  }
  return raw;
}

function workingDirectory() {
  const workspace = process.env.GITHUB_WORKSPACE || process.cwd();
  const requested = (process.env.GIT_SLOP_WORKING_DIRECTORY || ".").trim();
  const candidate = isAbsolute(requested) ? requested : resolve(workspace, requested);
  if (!existsSync(candidate) || !statSync(candidate).isDirectory()) {
    throw new Error(`working directory does not exist: ${candidate}`);
  }
  return candidate;
}

function reportPaths(root) {
  const latest = join(root, ".slop", "latest");
  return {
    healthPath: join(latest, "health.md"),
    reportPath: join(latest, "report.json"),
    reportYamlPath: join(latest, "report.yaml"),
    summaryPath: join(latest, "summary.md"),
  };
}

function run(command, args, cwd, stdio = "inherit") {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    stdio,
  });
  if (result.error) {
    return {
      status: 2,
      stderr: result.error.message,
      stdout: "",
    };
  }
  return {
    status: result.status ?? 2,
    stderr: result.stderr || "",
    stdout: result.stdout || "",
  };
}

function reportFindingCount(reportPath) {
  if (!existsSync(reportPath)) {
    return 0;
  }
  try {
    const report = JSON.parse(readFileSync(reportPath, "utf8"));
    if (Array.isArray(report.health?.findings)) {
      return report.health.findings.length;
    }
    if (Array.isArray(report.findings)) {
      return report.findings.length;
    }
  } catch (error) {
    console.warn(`Unable to read finding count from report.json: ${error.message}`);
  }
  return 0;
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function appendHealthSummary(healthPath, inputs = {}) {
  const summaryTarget = process.env.GITHUB_STEP_SUMMARY;
  if (!summaryTarget) {
    console.warn("GITHUB_STEP_SUMMARY is unavailable; health.md was not appended.");
    return;
  }
  const health = readFileSync(healthPath, "utf8");
  const findArgs = ["git slop find", "--quiet"];
  if (inputs.allowShallow) findArgs.push("--allow-shallow");
  const scope = (process.env.GIT_SLOP_SCOPE || "").trim();
  if (scope) findArgs.push("--scope", shellQuote(scope));
  const reproduce = [
    "",
    "## Reproduce locally",
    "",
    "```bash",
    findArgs.join(" "),
    "git slop health --format markdown",
    "```",
  ].join("\n");
  writeFileSync(summaryTarget, `${health.trimEnd()}\n${reproduce}\n`, { flag: "a" });
}

function appendArtifactLink() {
  const summaryTarget = process.env.GITHUB_STEP_SUMMARY;
  const artifactUrl = (process.env.GIT_SLOP_ARTIFACT_URL || "").trim();
  if (summaryTarget && artifactUrl) {
    writeFileSync(summaryTarget, `\n[Download the bounded Git Slop artifact](${artifactUrl})\n`, {
      flag: "a",
    });
  }
}

function writeFallbackHealth(healthPath, message) {
  mkdirSync(dirname(healthPath), { recursive: true });
  const safeMessage = String(message).replace(/\r?\n/gu, " ");
  writeFileSync(
    healthPath,
    [
      "# Repository Health",
      "",
      "> [!CAUTION]",
      `> Git Slop could not generate repository-health analysis: ${safeMessage}`,
      "",
      "Review the Action log for installation, checkout-depth, or detector diagnostics.",
      "",
    ].join("\n"),
    "utf8",
  );
}

function validateInputs() {
  return {
    policy: enumValue("GIT_SLOP_POLICY", "advisory", ["advisory", "enforce"]),
    annotations: normalizedBoolean("GIT_SLOP_ANNOTATIONS", "true"),
    maxAnnotations: boundedInteger("GIT_SLOP_MAX_ANNOTATIONS", 10, 0, 50),
    uploadArtifact: normalizedBoolean("GIT_SLOP_UPLOAD_ARTIFACT", "true"),
    artifactContents: enumValue("GIT_SLOP_ARTIFACT_CONTENTS", "summary", [
      "summary",
      "report",
      "full",
    ]),
    retentionDays: boundedInteger("GIT_SLOP_RETENTION_DAYS", 14, 1, 90),
    prComment: normalizedBoolean("GIT_SLOP_PR_COMMENT", "false"),
    allowShallow: normalizedBoolean("GIT_SLOP_ALLOW_SHALLOW", "false"),
    failOnContextBand: optionalBand("GIT_SLOP_FAIL_ON_CONTEXT_BAND", [
      "compact",
      "healthy",
      "warning",
      "critical",
    ]),
    failOnSlopBand: optionalBand("GIT_SLOP_FAIL_ON_SLOP_BAND", [
      "low",
      "moderate",
      "high",
      "critical",
    ]),
  };
}

function regressionComparison(basePath, headPath, outputPath) {
  const base = JSON.parse(readFileSync(basePath, "utf8"));
  const head = JSON.parse(readFileSync(headPath, "utf8"));
  const byPath = (report) => new Map((report.files || []).map((file) => [file.path, file]));
  const baseFiles = byPath(base);
  const regressions = [];
  for (const [path, file] of byPath(head)) {
    const previous = baseFiles.get(path);
    const finding = ["warning", "critical", "refactor_required"].includes(file.context_band)
      || ["high", "critical"].includes(file.slop_band);
    const worsened = previous && Number(file.slop_score || 0) > Number(previous.slop_score || 0);
    if ((finding && !previous) || worsened) {
      regressions.push({
        path,
        status: previous ? "worsened" : "new",
        base_slop_score: previous?.slop_score ?? null,
        head_slop_score: file.slop_score ?? null,
        context_band: file.context_band,
        slop_band: file.slop_band,
      });
    }
  }
  regressions.sort((left, right) => String(left.path).localeCompare(String(right.path)));
  const payload = { schema_version: 1, command: "compare", regression_count: regressions.length, regressions };
  writeFileSync(outputPath, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
  return payload;
}

function analyze() {
  let cwd;
  let inputs;
  let analysisExitCode = 2;
  let failureMessage = "";
  const workspace = process.env.GITHUB_WORKSPACE || process.cwd();
  const requested = (process.env.GIT_SLOP_WORKING_DIRECTORY || ".").trim();
  const fallbackCwd = isAbsolute(requested) ? requested : resolve(workspace, requested);
  const fallbackRoot =
    existsSync(fallbackCwd) && statSync(fallbackCwd).isDirectory()
      ? fallbackCwd
      : resolve(process.env.RUNNER_TEMP || workspace, "git-slop-action-fallback");
  let { healthPath, reportPath, reportYamlPath, summaryPath } = reportPaths(fallbackRoot);
  let comparisonPath = "";

  try {
    inputs = validateInputs();
    const requestedCwd = workingDirectory();
    const gitWorktree = run("git", ["rev-parse", "--is-inside-work-tree"], requestedCwd, "pipe");
    if (gitWorktree.status !== 0 || gitWorktree.stdout.trim() !== "true") {
      throw new Error("working-directory is not a Git worktree");
    }
    const gitRoot = run("git", ["rev-parse", "--show-toplevel"], requestedCwd, "pipe");
    if (gitRoot.status !== 0 || !gitRoot.stdout.trim()) {
      throw new Error(`could not resolve the Git worktree root: ${gitRoot.stderr.trim()}`);
    }
    cwd = resolve(gitRoot.stdout.trim());
    ({ healthPath, reportPath, reportYamlPath, summaryPath } = reportPaths(cwd));
    const shallow = run("git", ["rev-parse", "--is-shallow-repository"], cwd, "pipe");
    if (shallow.status !== 0) {
      throw new Error(`could not inspect Git history: ${shallow.stderr.trim()}`);
    }
    if (shallow.stdout.trim() === "true" && !inputs.allowShallow) {
      throw new Error(
        "the checkout is shallow; use actions/checkout with fetch-depth: 0 for accurate history analysis",
      );
    }

    const binary = (process.env.GIT_SLOP_BINARY || "").trim();
    if (!binary || !existsSync(binary)) {
      const installOutcome = process.env.GIT_SLOP_INSTALL_OUTCOME || "unknown";
      throw new Error(`verified git-slop binary is unavailable (install outcome: ${installOutcome})`);
    }
    console.log("Running git-slop find once");
    // Repository-controlled filenames appear in detector output. Keep raw
    // stdout/stderr away from the workflow-command parser; the escaped health
    // summary and annotation renderer are the supported publication surfaces.
    const findArgs = ["find", "--quiet"];
    if (inputs.allowShallow) findArgs.push("--allow-shallow");
    const scope = (process.env.GIT_SLOP_SCOPE || "").trim();
    if (scope) findArgs.push("--scope", scope);
    const result = run(binary, findArgs, cwd, "pipe");
    analysisExitCode = result.status;
    if (analysisExitCode !== 0) {
      throw new Error(`git-slop find exited with status ${analysisExitCode}`);
    }
    if (!existsSync(reportPath) || !existsSync(healthPath)) {
      analysisExitCode = 2;
      throw new Error("git-slop find did not write .slop/latest/report.json and health.md");
    }
  } catch (error) {
    failureMessage = error instanceof Error ? error.message : String(error);
    console.error(`git-slop Action analysis failed: ${failureMessage}`);
    writeFallbackHealth(healthPath, failureMessage);
  }

  if (!existsSync(healthPath)) {
    failureMessage ||= "health.md is unavailable";
    analysisExitCode = 2;
    writeFallbackHealth(healthPath, failureMessage);
  }
  const safeInputs = inputs || {
    policy: "advisory",
    annotations: false,
    maxAnnotations: 0,
    uploadArtifact: true,
    artifactContents: "summary",
    retentionDays: 14,
    prComment: false,
    allowShallow: false,
    failOnContextBand: "",
    failOnSlopBand: "",
  };
  appendHealthSummary(healthPath, safeInputs);
  let findingCount = analysisExitCode === 0 ? reportFindingCount(reportPath) : 0;
  const baselineInput = (process.env.GIT_SLOP_BASELINE_REPORT || "").trim();
  if (analysisExitCode === 0 && baselineInput) {
    const baselinePath = isAbsolute(baselineInput) ? baselineInput : resolve(cwd, baselineInput);
    if (!existsSync(baselinePath)) throw new Error(`baseline report does not exist: ${baselinePath}`);
    comparisonPath = join(dirname(reportPath), "comparison.json");
    findingCount = regressionComparison(baselinePath, reportPath, comparisonPath).regression_count;
  }
  const artifactContents = analysisExitCode === 0 ? safeInputs.artifactContents : "summary";
  setOutput("analysis-exit-code", analysisExitCode);
  setOutput("finding-count", findingCount);
  setOutput("comparison-path", comparisonPath);
  setOutput("health-path", healthPath);
  setOutput("report-path", analysisExitCode === 0 ? reportPath : "");
  setOutput("report-yaml-path", analysisExitCode === 0 ? reportYamlPath : "");
  setOutput("summary-path", analysisExitCode === 0 ? summaryPath : "");
  setOutput("working-directory", cwd || fallbackCwd);
  setOutput("policy", safeInputs.policy);
  setOutput("annotations-enabled", safeInputs.annotations);
  setOutput("max-annotations", safeInputs.maxAnnotations);
  setOutput("upload-enabled", safeInputs.uploadArtifact);
  setOutput("artifact-contents", artifactContents);
  setOutput("retention-days", safeInputs.retentionDays);
  setOutput("comment-enabled", safeInputs.prComment);
  setOutput("fail-on-context-band", safeInputs.failOnContextBand);
  setOutput("fail-on-slop-band", safeInputs.failOnSlopBand);
}

function annotate() {
  const binary = (process.env.GIT_SLOP_BINARY || "").trim();
  const reportPath = (process.env.GIT_SLOP_REPORT_PATH || "").trim();
  const cwd = (process.env.GIT_SLOP_WORKING_DIRECTORY_RESOLVED || "").trim();
  const findingCount = boundedInteger("GIT_SLOP_FINDING_COUNT", 0, 0, Number.MAX_SAFE_INTEGER);
  const maximum = boundedInteger("GIT_SLOP_MAX_ANNOTATIONS", 10, 0, 50);
  const annotationCount = Math.min(findingCount, maximum);
  setOutput("annotation-count", annotationCount);
  if (maximum === 0 || findingCount === 0) {
    console.log("No Git Slop annotations requested.");
    return;
  }
  const comparisonPath = (process.env.GIT_SLOP_COMPARISON_PATH || "").trim();
  if (comparisonPath && existsSync(comparisonPath)) {
    const comparison = JSON.parse(readFileSync(comparisonPath, "utf8"));
    for (const finding of comparison.regressions.slice(0, maximum)) {
      const safePath = String(finding.path).replace(/[\r\n\x1b]/gu, " ").replace(/%/gu, "%25").replace(/,/gu, "%2C");
      console.log(`::warning file=${safePath}::Git Slop ${finding.status}: score ${finding.base_slop_score ?? "new"} -> ${finding.head_slop_score}`);
    }
    return;
  }
  const result = run(
    binary,
    ["health", "--report", reportPath, "--format", "github", "--max-annotations", String(maximum)],
    cwd,
  );
  if (result.status !== 0) {
    throw new Error(`git-slop health annotation rendering exited with status ${result.status}`);
  }
}

function artifacts() {
  const mode = enumValue("GIT_SLOP_ARTIFACT_CONTENTS", "summary", [
    "summary",
    "report",
    "full",
  ]);
  const healthPath = (process.env.GIT_SLOP_HEALTH_PATH || "").trim();
  const reportPath = (process.env.GIT_SLOP_REPORT_PATH || "").trim();
  const reportYamlPath = (process.env.GIT_SLOP_REPORT_YAML_PATH || "").trim();
  const summaryPath = (process.env.GIT_SLOP_SUMMARY_PATH || "").trim();
  const allowed = {
    summary: [healthPath],
    report: [healthPath, reportPath],
    full: [healthPath, summaryPath, reportPath, reportYamlPath],
  };
  const paths = allowed[mode].filter((candidate) => candidate && existsSync(candidate));
  if (!paths.includes(healthPath)) {
    throw new Error("bounded artifact selection requires health.md");
  }
  setOutput("paths", paths.join("\n"));
  console.log(`Artifact mode ${mode} selected ${paths.length} explicit file(s).`);
}

function finalize() {
  const analysisExitCode = boundedInteger("GIT_SLOP_ANALYSIS_EXIT_CODE", 2, 0, 255);
  if (analysisExitCode !== 0) {
    setOutput("status", "error");
    setOutput("policy-exit-code", 2);
    throw new Error(`git-slop analysis failed with status ${analysisExitCode}`);
  }

  const publicationOutcomes = {
    annotations: (process.env.GIT_SLOP_ANNOTATE_OUTCOME || "").trim(),
    "artifact selection": (process.env.GIT_SLOP_ARTIFACT_SELECTION_OUTCOME || "").trim(),
    "artifact upload": (process.env.GIT_SLOP_UPLOAD_OUTCOME || "").trim(),
    "pull request comment": (process.env.GIT_SLOP_COMMENT_OUTCOME || "").trim(),
  };
  const failedPublications = Object.entries(publicationOutcomes)
    .filter(([, outcome]) => outcome === "failure" || outcome === "cancelled")
    .map(([name]) => name);
  if (failedPublications.length > 0) {
    setOutput("status", "error");
    setOutput("policy-exit-code", 2);
    throw new Error(`Git Slop publication failed: ${failedPublications.join(", ")}`);
  }
  appendArtifactLink();

  const policy = enumValue("GIT_SLOP_POLICY", "advisory", ["advisory", "enforce"]);
  if (policy === "advisory") {
    setOutput("status", "advisory");
    setOutput("policy-exit-code", 0);
    console.log("Git Slop policy is advisory; findings do not fail the job.");
    return;
  }

  const binary = (process.env.GIT_SLOP_BINARY || "").trim();
  const reportPath = (process.env.GIT_SLOP_REPORT_PATH || "").trim();
  const cwd = (process.env.GIT_SLOP_WORKING_DIRECTORY_RESOLVED || "").trim();
  const contextBand = optionalBand("GIT_SLOP_FAIL_ON_CONTEXT_BAND", [
    "compact",
    "healthy",
    "warning",
    "critical",
  ]);
  const slopBand = optionalBand("GIT_SLOP_FAIL_ON_SLOP_BAND", [
    "low",
    "moderate",
    "high",
    "critical",
  ]);
  const args = ["check", "--report", reportPath];
  if (contextBand) {
    args.push("--fail-on-context-band", contextBand);
  }
  if (slopBand) {
    args.push("--fail-on-slop-band", slopBand);
  }
  const result = run(binary, args, cwd, "pipe");
  const status = result.status === 0 ? "pass" : result.status === 1 ? "findings" : "error";
  setOutput("status", status);
  setOutput("policy-exit-code", result.status);
  if (result.status !== 0) {
    throw new Error(
      result.status === 1
        ? "Git Slop policy found repository health violations."
        : `git-slop check failed with status ${result.status}`,
    );
  }
}

const command = process.argv[2];
try {
  if (command === "analyze") {
    analyze();
  } else if (command === "annotate") {
    annotate();
  } else if (command === "artifacts") {
    artifacts();
  } else if (command === "finalize") {
    finalize();
  } else {
    throw new Error(`unknown action runner command ${JSON.stringify(command)}`);
  }
} catch (error) {
  console.error(`git-slop Action ${command || "runner"} failed: ${error.message}`);
  process.exitCode = 1;
}
