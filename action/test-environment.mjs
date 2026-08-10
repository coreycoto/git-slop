const isolatedPrefixes = ["FAKE_", "GITHUB_", "GIT_SLOP_"];
const isolatedRunnerVariables = new Set(["RUNNER_TEMP", "RUNNER_TOOL_CACHE"]);

export function isolatedActionEnvironment(overrides = {}) {
  const environment = { ...process.env };
  for (const name of Object.keys(environment)) {
    if (
      isolatedPrefixes.some((prefix) => name.startsWith(prefix)) ||
      isolatedRunnerVariables.has(name)
    ) {
      delete environment[name];
    }
  }
  return { ...environment, ...overrides };
}
