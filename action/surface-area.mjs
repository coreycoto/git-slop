import { existsSync, readFileSync } from "node:fs";

const DIMENSIONS = [
  ["Tracked files", () => true],
  ["Documentation files", (path) => path.startsWith("docs/") || /\.(?:md|mdx|rst|adoc)$/iu.test(path)],
  ["Test files", (path) => /(?:^|\/)(?:tests?|__tests__)(?:\/|$)/iu.test(path) || /(?:^|\/)[^/]+[._-](?:test|spec)\.[^/]+$/iu.test(path)],
  ["GitHub workflow files", (path) => /^\.github\/workflows\/[^/]+\.ya?ml$/iu.test(path)],
  ["Machine-schema files", (path) => /(?:^|\/)schemas?(?:\/|$)/iu.test(path) && /\.(?:json|ya?ml)$/iu.test(path)],
  ["Agent skill definitions", (path) => /(?:^|\/)SKILL\.md$/u.test(path)],
  ["CLI and distribution files", (path) => path.startsWith("docs/cli-reference/") || /(?:^|\/)(?:man|completions)(?:\/|$)/u.test(path)],
  ["Tooling and automation files", (path) => /^(?:action|scripts|tools|xtask|\.github\/codex)(?:\/|$)/u.test(path)],
];
const INVENTORY_MAX_BUFFER = 64 * 1024 * 1024;

function markdownEscape(value) {
  return String(value)
    .replace(/[\r\n\x00-\x1f\x7f]/gu, " ")
    .replaceAll("\\", "\\\\")
    .replaceAll("|", "\\|")
    .replaceAll("`", "\\`");
}

function signed(value) {
  return value > 0 ? `+${value}` : String(value);
}

function scopePrefix(scope) {
  const normalized = String(scope || "").trim().replaceAll("\\", "/").replace(/^\.\//u, "").replace(/\/+$/u, "");
  if (!normalized || normalized === ".") return "";
  if (normalized.startsWith("/") || normalized.split("/").some((part) => part === "..")) {
    throw new Error(`scope is not a safe repository-relative path: ${JSON.stringify(scope)}`);
  }
  return normalized;
}

function scopedPaths(paths, scope) {
  const prefix = scopePrefix(scope);
  if (!prefix) return paths;
  return paths
    .filter((path) => path === prefix || path.startsWith(`${prefix}/`))
    .map((path) => path === prefix ? "." : path.slice(prefix.length + 1));
}

function nulPaths(output) {
  return String(output || "").split("\0").filter(Boolean);
}

function commandPaths(run, cwd, command, args, label) {
  const result = run(command, args, cwd, "pipe", { maxBuffer: INVENTORY_MAX_BUFFER });
  if (result.status !== 0) {
    throw new Error(`${label} failed with status ${result.status}: ${String(result.stderr || "").trim() || "no diagnostic"}`);
  }
  return nulPaths(result.stdout);
}

function headPaths(run, cwd, scope) {
  return scopedPaths(commandPaths(run, cwd, "git", ["ls-files", "-z", "--cached"], "tracked-file inventory"), scope);
}

function revisionPaths(run, cwd, revision, scope) {
  return scopedPaths(commandPaths(run, cwd, "git", ["ls-tree", "-r", "-z", "--name-only", revision], "baseline tree inventory"), scope);
}

function resolveRevision(run, cwd, revision) {
  if (!revision) return "";
  const result = run("git", ["rev-parse", "--verify", "--end-of-options", `${revision}^{commit}`], cwd, "pipe");
  const resolved = String(result.stdout || "").trim();
  return result.status === 0 && /^[a-f0-9]{40}$/u.test(resolved) ? resolved : "";
}

function pullRequestBaseRevision(eventPath) {
  if (!eventPath || !existsSync(eventPath)) return "";
  const event = JSON.parse(readFileSync(eventPath, "utf8"));
  const revision = event?.pull_request?.base?.sha;
  return typeof revision === "string" && /^[a-f0-9]{40}$/u.test(revision) ? revision : "";
}

function topLevelRootCount(paths) {
  return new Set(paths.map((path) => path === "." ? "." : path.split("/", 1)[0])).size;
}

function counts(paths) {
  const values = new Map(DIMENSIONS.map(([label, predicate]) => [label, paths.filter(predicate).length]));
  values.set("Top-level namespaces", topLevelRootCount(paths));
  return values;
}

function pathMovement(run, cwd, baseRevision, basePaths, currentPaths, scope) {
  const base = new Set(basePaths);
  const head = new Set(currentPaths);
  const added = currentPaths.filter((path) => !base.has(path)).length;
  const removed = basePaths.filter((path) => !head.has(path)).length;
  const changedPaths = scopedPaths(
    commandPaths(run, cwd, "git", ["diff", "--name-only", "-z", baseRevision, "--"], "pull-request path comparison"),
    scope,
  );
  const retainedChanged = new Set(changedPaths.filter((path) => base.has(path) && head.has(path))).size;
  return { added, removed, retainedChanged };
}

export function buildSurfaceAreaMarkdown(run, cwd, options = {}) {
  const scope = scopePrefix(options.scope || process.env.GIT_SLOP_SCOPE || "");
  const currentPaths = headPaths(run, cwd, scope);
  const head = counts(currentPaths);
  const requestedBase = options.baseRevision
    || pullRequestBaseRevision(options.eventPath || process.env.GITHUB_EVENT_PATH || "")
    || String(options.baselineRef || process.env.GIT_SLOP_BASELINE_REF || "").trim();
  const baseRevision = resolveRevision(run, cwd, requestedBase);
  const lines = [
    "## Surface-area ledger",
    "",
    "> [!NOTE]",
    "> Experimental, advisory evidence only. Dimensions may overlap; there is no composite score, and growth is not itself a finding. Review each delta against demonstrated user value.",
    "",
  ];

  if (baseRevision) {
    const basePaths = revisionPaths(run, cwd, baseRevision, scope);
    const base = counts(basePaths);
    const movement = pathMovement(run, cwd, baseRevision, basePaths, currentPaths, scope);
    lines.push(
      "| Observed dimension | Base | Head | Delta |",
      "| --- | ---: | ---: | ---: |",
    );
    for (const label of ["Tracked files", "Top-level namespaces", ...DIMENSIONS.slice(1).map(([name]) => name)]) {
      lines.push(`| ${label} | ${base.get(label)} | ${head.get(label)} | ${signed(head.get(label) - base.get(label))} |`);
    }
    lines.push(
      "",
      `- **Path movement:** ${movement.added} added, ${movement.removed} removed, ${movement.retainedChanged} retained path(s) changed`,
      `- **Baseline revision:** \`${baseRevision}\``,
    );
  } else {
    lines.push(
      "| Observed dimension | Head |",
      "| --- | ---: |",
    );
    for (const label of ["Tracked files", "Top-level namespaces", ...DIMENSIONS.slice(1).map(([name]) => name)]) {
      lines.push(`| ${label} | ${head.get(label)} |`);
    }
    lines.push(
      "",
      requestedBase
        ? "> Baseline delta unavailable because the requested revision is not present in this checkout."
        : "> Baseline delta unavailable because this run has no pull-request base revision.",
    );
  }
  lines.push(
    `- **Scope:** ${scope ? `\`${markdownEscape(scope)}\`` : "whole repository"}`,
    "- **Experiment review:** reassess after 10 pull requests; consolidate or remove the ledger if it has not changed a reviewer decision or its path dimensions routinely mislead.",
    "",
  );
  return lines.join("\n");
}
