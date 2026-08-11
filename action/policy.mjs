export function evaluateAbsolutePolicy(run, binary, cwd, reportPath, inputs) {
  const args = [
    "check",
    "--report",
    reportPath,
    "--format",
    "json",
    "--evaluate-only",
  ];
  if (inputs.failOnContextBand) {
    args.push("--fail-on-context-band", inputs.failOnContextBand);
  }
  if (inputs.failOnSlopBand) {
    args.push("--fail-on-slop-band", inputs.failOnSlopBand);
  }
  const result = run(binary, args, cwd, "pipe");
  if (result.status !== 0) {
    throw new Error(`git-slop check --evaluate-only failed with status ${result.status}`);
  }
  let payload;
  try {
    payload = JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(`git-slop check returned invalid JSON: ${error.message}`);
  }
  if (
    payload?.command !== "check"
    || !Number.isSafeInteger(payload.finding_count)
    || payload.finding_count < 0
    || typeof payload.passed !== "boolean"
  ) {
    throw new Error("git-slop check returned an incompatible policy payload");
  }
  if (payload.passed !== (payload.finding_count === 0)) {
    throw new Error("git-slop check returned inconsistent passed and finding_count values");
  }
  return payload;
}

export function emittedAnnotationCount(output) {
  return String(output)
    .split(/\r?\n/u)
    .filter((line) => /^::(?:notice|warning|error)(?: |::)/u.test(line))
    .length;
}
