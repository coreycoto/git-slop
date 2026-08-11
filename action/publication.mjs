import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { emittedAnnotationCount } from "./policy.mjs";
import { boundedInteger, enumValue, run, setOutput } from "./runtime.mjs";

function githubPropertyEscape(value) {
  return String(value)
    .replace(/%/gu, "%25")
    .replace(/\r/gu, "%0D")
    .replace(/\n/gu, "%0A")
    .replace(/:/gu, "%3A")
    .replace(/,/gu, "%2C");
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

export function annotate() {
  const binary = (process.env.GIT_SLOP_BINARY || "").trim();
  const reportPath = (process.env.GIT_SLOP_REPORT_PATH || "").trim();
  const cwd = (process.env.GIT_SLOP_WORKING_DIRECTORY_RESOLVED || "").trim();
  const maximum = boundedInteger("GIT_SLOP_MAX_ANNOTATIONS", 10, 0, 50);
  if (maximum === 0) {
    setOutput("annotation-count", 0);
    console.log("No Git Slop annotations requested.");
    return;
  }
  const comparisonPath = (process.env.GIT_SLOP_COMPARISON_PATH || "").trim();
  if (comparisonPath && existsSync(comparisonPath)) {
    const comparison = JSON.parse(readFileSync(comparisonPath, "utf8"));
    const findings = comparison.regressions.slice(0, maximum);
    for (const finding of findings) {
      const safePath = githubPropertyEscape(finding.path);
      const level = ["error", "warning", "notice"].includes(finding.severity) ? finding.severity : "warning";
      console.log(`::${level} file=${safePath}::Git Slop ${finding.status}: score ${finding.base_slop_score ?? "new"} -> ${finding.head_slop_score}`);
    }
    setOutput("annotation-count", findings.length);
    return;
  }
  const result = run(
    binary,
    ["health", "--report", reportPath, "--format", "github", "--max-annotations", String(maximum)],
    cwd,
    "pipe",
  );
  if (result.status !== 0) {
    throw new Error(`git-slop health annotation rendering exited with status ${result.status}`);
  }
  process.stdout.write(result.stdout);
  setOutput("annotation-count", emittedAnnotationCount(result.stdout));
}

export function artifacts() {
  const mode = enumValue("GIT_SLOP_ARTIFACT_CONTENTS", "summary", ["summary", "report", "full"]);
  const healthPath = (process.env.GIT_SLOP_HEALTH_PATH || "").trim();
  const analysisErrorPath = (process.env.GIT_SLOP_ANALYSIS_ERROR_PATH || "").trim();
  const compressedReportPath = (process.env.GIT_SLOP_COMPRESSED_REPORT_PATH || "").trim();
  const reportPath = (process.env.GIT_SLOP_REPORT_PATH || "").trim();
  const reportYamlPath = (process.env.GIT_SLOP_REPORT_YAML_PATH || "").trim();
  const summaryPath = (process.env.GIT_SLOP_SUMMARY_PATH || "").trim();
  const comparisonPath = (process.env.GIT_SLOP_COMPARISON_PATH || "").trim();
  const comparisonErrorPath = (process.env.GIT_SLOP_COMPARISON_ERROR_PATH || "").trim();
  const allowed = {
    summary: [healthPath, analysisErrorPath, comparisonErrorPath],
    report: [healthPath, reportPath, compressedReportPath, analysisErrorPath, comparisonPath, comparisonErrorPath],
    full: [healthPath, summaryPath, reportPath, compressedReportPath, reportYamlPath, analysisErrorPath, comparisonPath, comparisonErrorPath],
  };
  const paths = allowed[mode].filter((candidate) => candidate && existsSync(candidate));
  if (!paths.includes(healthPath)) throw new Error("bounded artifact selection requires health.md");
  setOutput("paths", paths.join("\n"));
  console.log(`Artifact mode ${mode} selected ${paths.length} explicit file(s).`);
}

export function finalize() {
  const analysisExitCode = boundedInteger("GIT_SLOP_ANALYSIS_EXIT_CODE", 2, 0, 255);
  if (analysisExitCode !== 0) {
    setOutput("status", "error");
    setOutput("policy-exit-code", 2);
    throw new Error(`git-slop analysis failed with status ${analysisExitCode}`);
  }
  const baselineStatus = (process.env.GIT_SLOP_BASELINE_STATUS || "not_evaluated").trim();
  if (baselineStatus === "error") {
    setOutput("status", "error");
    setOutput("policy-exit-code", 2);
    throw new Error("Git Slop head analysis succeeded, but baseline comparison failed; head artifacts were preserved.");
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
  const enforcement = enumValue("GIT_SLOP_ENFORCEMENT", "absolute", ["absolute", "regression"]);
  const countName = enforcement === "regression" ? "GIT_SLOP_REGRESSION_COUNT" : "GIT_SLOP_ABSOLUTE_FINDING_COUNT";
  const findingCount = boundedInteger(countName, 0, 0, Number.MAX_SAFE_INTEGER);
  const status = findingCount === 0 ? "pass" : "findings";
  setOutput("status", status);
  setOutput("policy-exit-code", findingCount === 0 ? 0 : 1);
  if (findingCount > 0) {
    const noun = enforcement === "regression" ? "repository-health regression(s)" : "repository health violation(s)";
    throw new Error(`Git Slop policy found ${findingCount} ${noun}.`);
  }
  console.log(enforcement === "regression"
    ? "Git Slop regression policy found no new or worsened findings."
    : "Git Slop policy found no repository health violations.");
}
