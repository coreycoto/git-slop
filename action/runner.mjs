import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, resolve } from "node:path";
import {
  baselineReportFromRef as materializeBaselineReport,
  baselineRevisionStatus,
  regressionComparison as compareBaselineReports,
} from "./baseline.mjs";
import { evaluateAbsolutePolicy } from "./policy.mjs";
import { annotate, artifacts, finalize } from "./publication.mjs";
import { boundedInteger, enumValue, run, safeLogText, setOutput } from "./runtime.mjs";

function normalizedBoolean(name, fallback) {
  const raw = (process.env[name] || fallback).trim().toLowerCase();
  if (raw !== "true" && raw !== "false") {
    throw new Error(`${name} must be true or false, received ${JSON.stringify(raw)}`);
  }
  return raw === "true";
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
  return reportPathsFromOutput(join(root, ".slop"));
}

function reportPathsFromOutput(outputRoot) {
  const latest = join(outputRoot, "latest");
  return {
    analysisErrorPath: join(latest, "analysis-error.md"),
    compressedGzipPath: join(latest, "report.json.gz"),
    compressedZstdPath: join(latest, "report.json.zst"),
    healthPath: join(latest, "health.md"),
    reportPath: join(latest, "report.json"),
    reportYamlPath: join(latest, "report.yaml"),
    summaryPath: join(latest, "summary.md"),
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

function appendComparisonSummary(comparisonPath) {
  const summaryTarget = process.env.GITHUB_STEP_SUMMARY;
  if (!summaryTarget || !comparisonPath || !existsSync(comparisonPath)) return;
  const comparison = JSON.parse(readFileSync(comparisonPath, "utf8"));
  const mismatches = Array.isArray(comparison.compatibility_mismatches)
    ? comparison.compatibility_mismatches
    : [];
  const lines = [
    "",
    "## Baseline comparison",
    "",
    `- Regressions: **${comparison.summary?.regression_count ?? 0}**`,
    `- Baseline status: **${comparison.baseline_status || (comparison.baseline_compatible === true ? "compatible" : "incompatible")}**`,
  ];
  if (mismatches.length > 0) {
    const inline = (value) => String(value).replace(/[\r\n\x00-\x1f\x7f]/gu, " ").replaceAll("`", "\\`");
    lines.push("", "> [!WARNING]", "> Baseline compatibility was forced. Exact mismatches:");
    for (const mismatch of mismatches) {
      lines.push(`> - \`${inline(mismatch.pointer)}\`: base \`${inline(JSON.stringify(mismatch.base))}\`, head \`${inline(JSON.stringify(mismatch.head))}\``);
    }
  }
  writeFileSync(summaryTarget, `${lines.join("\n")}\n`, { flag: "a" });
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
    enforcement: enumValue("GIT_SLOP_ENFORCEMENT", "absolute", ["absolute", "regression"]),
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
    baselineForce: normalizedBoolean("GIT_SLOP_BASELINE_FORCE", "false"),
    maxBaselineAgeDays: boundedInteger("GIT_SLOP_MAX_BASELINE_AGE_DAYS", 30, 1, 3650),
    requireBaselineAncestor: normalizedBoolean("GIT_SLOP_REQUIRE_BASELINE_ANCESTOR", "true"),
    reportProfile: enumValue("GIT_SLOP_REPORT_PROFILE", "standard", ["compact", "standard", "full-evidence"]),
    compression: enumValue("GIT_SLOP_COMPRESSION", "none", ["none", "gzip", "zstd"]),
    tokenCache: normalizedBoolean("GIT_SLOP_TOKEN_CACHE", "false"),
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
  let {
    analysisErrorPath,
    compressedGzipPath,
    compressedZstdPath,
    healthPath,
    reportPath,
    reportYamlPath,
    summaryPath,
  } = reportPaths(fallbackRoot);
  let compressedReportPath = "";
  let reportGenerated = false;
  let comparisonPath = "";
  let comparisonErrorPath = "";

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
    const runnerTemp = (process.env.RUNNER_TEMP || "").trim();
    const outputRoot = runnerTemp ? resolve(runnerTemp, "git-slop-action", "reports") : join(cwd, ".slop");
    const stateRoot = runnerTemp ? resolve(runnerTemp, "git-slop-action", "state") : join(cwd, ".slop");
    ({
      analysisErrorPath,
      compressedGzipPath,
      compressedZstdPath,
      healthPath,
      reportPath,
      reportYamlPath,
      summaryPath,
    } = reportPathsFromOutput(outputRoot));
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
    findArgs.push("--report-profile", inputs.reportProfile, "--compression", inputs.compression);
    if (runnerTemp) findArgs.push("--output-dir", outputRoot, "--state-dir", stateRoot);
    if (!inputs.tokenCache) findArgs.push("--no-cache");
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
    reportGenerated = true;
    compressedReportPath = inputs.compression === "gzip"
      ? compressedGzipPath
      : inputs.compression === "zstd"
        ? compressedZstdPath
        : "";
    if (compressedReportPath && !existsSync(compressedReportPath)) {
      analysisExitCode = 2;
      throw new Error(`git-slop find did not write requested ${inputs.compression} report`);
    }
    const validation = run(binary, ["report", "validate", reportPath], cwd, "pipe");
    if (validation.status !== 0) {
      analysisExitCode = 2;
      throw new Error(
        `generated report failed immediate validation with status ${validation.status}: ${validation.stderr.trim()}`,
      );
    }
  } catch (error) {
    failureMessage = error instanceof Error ? error.message : String(error);
    console.error(`git-slop Action analysis failed: ${safeLogText(failureMessage)}`);
    mkdirSync(dirname(analysisErrorPath), { recursive: true });
    writeFileSync(
      analysisErrorPath,
      `# Git Slop analysis error\n\n${failureMessage.replace(/\r?\n/gu, " ")}\n`,
      "utf8",
    );
    if (!reportGenerated || !existsSync(healthPath)) writeFallbackHealth(healthPath, failureMessage);
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
    enforcement: "absolute",
    baselineForce: false,
    maxBaselineAgeDays: 30,
    requireBaselineAncestor: true,
    tokenCache: false,
    failOnContextBand: "",
    failOnSlopBand: "",
  };
  const healthFindingCount = analysisExitCode === 0 ? reportFindingCount(reportPath) : 0;
  let absolutePolicyFindingCount = healthFindingCount;
  if (analysisExitCode === 0) {
    try {
      absolutePolicyFindingCount = evaluateAbsolutePolicy(
        run,
        (process.env.GIT_SLOP_BINARY || "").trim(),
        cwd,
        reportPath,
        safeInputs,
      ).finding_count;
    } catch (error) {
      analysisExitCode = 2;
      failureMessage = error instanceof Error ? error.message : String(error);
      console.error(`git-slop Action report consumption failed: ${safeLogText(failureMessage)}`);
      writeFileSync(
        analysisErrorPath,
        `# Git Slop report consumption error\n\n${failureMessage.replace(/\r?\n/gu, " ")}\n`,
        "utf8",
      );
    }
  }
  let selectedPolicyFindingCount = absolutePolicyFindingCount;
  let regressionCount = 0;
  let baselineStatus = "not_evaluated";
  const baselineInput = (process.env.GIT_SLOP_BASELINE_REPORT || "").trim();
  const baselineRef = (process.env.GIT_SLOP_BASELINE_REF || "").trim();
  try {
    if (baselineInput && baselineRef) {
      throw new Error("baseline-report and baseline-ref are mutually exclusive");
    }
    if (analysisExitCode === 0 && (baselineInput || baselineRef)) {
      let baselinePath = baselineInput
        ? (isAbsolute(baselineInput) ? baselineInput : resolve(cwd, baselineInput))
        : "";
      let cleanupBaseline = () => {};
      let materialization = null;
      if (baselineRef) {
        const materialized = materializeBaselineReport(
          run,
          reportPaths,
          (process.env.GIT_SLOP_BINARY || "").trim(),
          cwd,
          baselineRef,
          safeInputs,
        );
        baselinePath = materialized.path;
        cleanupBaseline = materialized.cleanup;
        materialization = {
          reference: baselineRef,
          resolved_revision: materialized.resolvedRevision,
          isolated_worktree: true,
          copied_head_config: materialized.copiedHeadConfig,
        };
      }
      if (!existsSync(baselinePath)) throw new Error(`baseline report does not exist: ${baselinePath}`);
      comparisonPath = join(dirname(reportPath), "comparison.json");
      let comparison;
      try {
        comparison = compareBaselineReports(
          run,
          (process.env.GIT_SLOP_BINARY || "").trim(), cwd, baselinePath, reportPath,
          comparisonPath, safeInputs.baselineForce,
        );
      } finally {
        cleanupBaseline();
      }
      const generatedAt = Date.parse(comparison.base_report?.generated_at || "");
      if (!Number.isFinite(generatedAt)) {
        throw new Error("baseline report is missing a valid generated_at timestamp");
      }
      const ageDays = (Date.now() - generatedAt) / 86_400_000;
      if (ageDays < -1 / 24) {
        throw new Error("baseline report generated_at is implausibly in the future");
      }
      if (ageDays > safeInputs.maxBaselineAgeDays) {
        throw new Error(
          `baseline report is ${Math.floor(ageDays)} days old; maximum is ${safeInputs.maxBaselineAgeDays}`,
        );
      }
      comparison.baseline_revision = baselineRevisionStatus(run, cwd, comparison);
      if (
        safeInputs.enforcement === "regression" &&
        safeInputs.requireBaselineAncestor &&
        !safeInputs.baselineForce &&
        comparison.baseline_revision.ancestry !== "ancestor"
      ) {
        throw new Error(
          `baseline revision must be an ancestor of the head for regression enforcement; observed ${comparison.baseline_revision.ancestry}`,
        );
      }
      comparison.baseline_materialization = materialization;
      writeFileSync(comparisonPath, `${JSON.stringify(comparison)}\n`, "utf8");
      regressionCount = comparison.summary.regression_count;
      baselineStatus = comparison.baseline_status || (comparison.baseline_compatible ? "compatible" : "forced");
      if (safeInputs.enforcement === "regression") selectedPolicyFindingCount = regressionCount;
    }
    if (analysisExitCode === 0 && safeInputs.enforcement === "regression" && !baselineInput && !baselineRef) {
      throw new Error("enforcement=regression requires baseline-report or baseline-ref");
    }
  } catch (error) {
    failureMessage = error instanceof Error ? error.message : String(error);
    baselineStatus = "error";
    selectedPolicyFindingCount = absolutePolicyFindingCount;
    regressionCount = 0;
    comparisonPath = "";
    comparisonErrorPath = join(dirname(reportPath), "comparison-error.md");
    writeFileSync(
      comparisonErrorPath,
      `# Baseline comparison error\n\nThe head analysis completed and its artifacts were preserved.\n\n${String(failureMessage).replace(/\r?\n/gu, " ")}\n`,
      "utf8",
    );
    console.error(`git-slop Action baseline analysis failed: ${safeLogText(failureMessage)}`);
  }
  appendHealthSummary(healthPath, safeInputs);
  appendComparisonSummary(comparisonPath);
  const artifactContents = analysisExitCode === 0
    ? safeInputs.artifactContents
    : reportGenerated && existsSync(reportPath)
      ? "report"
      : "summary";
  setOutput("analysis-exit-code", analysisExitCode);
  setOutput("finding-count", selectedPolicyFindingCount);
  setOutput("health-finding-count", healthFindingCount);
  setOutput("policy-finding-count", selectedPolicyFindingCount);
  setOutput("absolute-finding-count", absolutePolicyFindingCount);
  setOutput("selected-policy-finding-count", selectedPolicyFindingCount);
  setOutput("regression-count", regressionCount);
  setOutput("baseline-status", baselineStatus);
  setOutput("baseline-compatible", baselineStatus === "compatible");
  setOutput("comparison-path", comparisonPath);
  setOutput("comparison-error-path", comparisonErrorPath);
  setOutput("analysis-error-path", existsSync(analysisErrorPath) ? analysisErrorPath : "");
  setOutput("health-path", healthPath);
  setOutput("report-path", reportGenerated && existsSync(reportPath) ? reportPath : "");
  setOutput("compressed-report-path", reportGenerated && compressedReportPath && existsSync(compressedReportPath) ? compressedReportPath : "");
  setOutput("report-yaml-path", reportGenerated && existsSync(reportYamlPath) ? reportYamlPath : "");
  setOutput("summary-path", reportGenerated && existsSync(summaryPath) ? summaryPath : "");
  setOutput("working-directory", cwd || fallbackCwd);
  setOutput("policy", safeInputs.policy);
  setOutput("enforcement", safeInputs.enforcement);
  setOutput("annotations-enabled", safeInputs.annotations);
  setOutput("max-annotations", safeInputs.maxAnnotations);
  setOutput("upload-enabled", safeInputs.uploadArtifact);
  setOutput("artifact-contents", artifactContents);
  setOutput("retention-days", safeInputs.retentionDays);
  setOutput("comment-enabled", safeInputs.prComment);
  setOutput("fail-on-context-band", safeInputs.failOnContextBand);
  setOutput("fail-on-slop-band", safeInputs.failOnSlopBand);
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
  console.error(`git-slop Action ${command || "runner"} failed: ${safeLogText(error.message)}`);
  process.exitCode = 1;
}
