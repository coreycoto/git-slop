import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export function regressionComparison(run, binary, cwd, basePath, headPath, outputPath, force) {
  const args = ["compare", "--base", basePath, "--head", headPath, "--format", "json"];
  if (force) args.push("--force");
  const result = run(binary, args, cwd, "pipe");
  if (result.status !== 0) {
    throw new Error(`git-slop compare exited with status ${result.status}: ${result.stderr.trim() || "no diagnostic"}`);
  }
  const payload = JSON.parse(result.stdout);
  if (!Array.isArray(payload.regressions) || !Number.isSafeInteger(payload.summary?.regression_count)) {
    throw new Error("git-slop compare returned an incompatible comparison payload");
  }
  writeFileSync(outputPath, `${JSON.stringify(payload)}\n`, "utf8");
  return payload;
}

export function baselineReportFromRef(run, reportPaths, binary, cwd, reference, inputs) {
  if (!/^[A-Za-z0-9][A-Za-z0-9._/-]{0,199}(?:[~^][0-9]{0,6})*$/u.test(reference) || reference.includes("..")) {
    throw new Error(`baseline-ref is not a safe Git revision: ${JSON.stringify(reference)}`);
  }
  const resolved = run("git", ["rev-parse", "--verify", "--end-of-options", `${reference}^{commit}`], cwd, "pipe");
  if (resolved.status !== 0 || !/^[a-f0-9]{40}$/u.test(resolved.stdout.trim())) {
    throw new Error(`baseline-ref does not resolve to a commit: ${reference}`);
  }
  const temporaryRoot = mkdtempSync(join(process.env.RUNNER_TEMP || dirname(cwd), "git-slop-baseline-"));
  const temporary = join(temporaryRoot, "worktree");
  const added = run("git", ["worktree", "add", "--detach", temporary, resolved.stdout.trim()], cwd, "pipe");
  if (added.status !== 0) {
    rmSync(temporaryRoot, { recursive: true, force: true });
    throw new Error(`could not create baseline worktree: ${added.stderr.trim()}`);
  }
  const cleanup = () => {
    run("git", ["worktree", "remove", "--force", temporary], cwd, "pipe");
    rmSync(temporaryRoot, { recursive: true, force: true });
  };
  const args = ["find", "--quiet"];
  if (inputs.allowShallow) args.push("--allow-shallow");
  const scope = (process.env.GIT_SLOP_SCOPE || "").trim();
  if (scope) args.push("--scope", scope);
  const headConfig = join(cwd, ".slop", "config.yaml");
  if (existsSync(headConfig)) {
    const baselineConfig = join(temporary, ".slop", "config.yaml");
    mkdirSync(dirname(baselineConfig), { recursive: true });
    copyFileSync(headConfig, baselineConfig);
  }
  const result = run(binary, args, temporary, "pipe");
  const path = reportPaths(temporary).reportPath;
  if (result.status !== 0 || !existsSync(path)) {
    cleanup();
    throw new Error(`baseline scan at ${reference} failed with status ${result.status}`);
  }
  return { path, cleanup, resolvedRevision: resolved.stdout.trim(), copiedHeadConfig: existsSync(headConfig) };
}

export function baselineRevisionStatus(run, cwd, comparison) {
  const baseSha = String(comparison.base_report?.head_sha || "");
  const headSha = String(comparison.head_report?.head_sha || "");
  const valid = value => /^[a-f0-9]{40}$/u.test(value);
  const analyzedAt = Date.parse(comparison.base_report?.analyzed_revision_at || "");
  const revisionAgeDays = Number.isFinite(analyzedAt)
    ? Math.max(0, (Date.now() - analyzedAt) / 86_400_000)
    : null;
  if (!valid(baseSha) || !valid(headSha)) {
    return { base_sha: baseSha || null, head_sha: headSha || null, revision_age_days: revisionAgeDays, ancestry: "unknown", divergence: null };
  }
  const ancestor = run("git", ["merge-base", "--is-ancestor", baseSha, headSha], cwd, "pipe");
  const ancestry = ancestor.status === 0 ? "ancestor" : ancestor.status === 1 ? "not_ancestor" : "unknown";
  const divergence = run("git", ["rev-list", "--left-right", "--count", `${baseSha}...${headSha}`], cwd, "pipe");
  const counts = divergence.status === 0 ? divergence.stdout.trim().split(/\s+/u).map(Number) : [];
  return {
    base_sha: baseSha,
    head_sha: headSha,
    revision_age_days: revisionAgeDays,
    ancestry,
    divergence: counts.length === 2 && counts.every(Number.isSafeInteger)
      ? { baseline_only_commits: counts[0], head_only_commits: counts[1] }
      : null,
  };
}
