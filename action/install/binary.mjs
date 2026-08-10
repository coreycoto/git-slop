import { spawnSync } from "node:child_process";

export function createBinaryVerifier({ exactObject }) {
  function verifyInstalledVersion(binaryPath, version) {
    const result = spawnSync(binaryPath, ["version"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    if (result.status !== 0) {
      throw new Error(`installed binary failed its version smoke test: ${(result.stderr || "").trim()}`);
    }
    const output = (result.stdout || "").trim();
    if (output !== `git-slop ${version}`) {
      throw new Error(`installed binary reported an unexpected version: ${output}`);
    }
  }

  function verifyInstalledBuildInfo(binaryPath, version, revision) {
    const result = spawnSync(binaryPath, ["build-info", "--format", "json"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    if (result.status !== 0) {
      throw new Error(
        `installed binary failed its build-info smoke test: ${(result.stderr || "").trim()}`,
      );
    }
    let buildInfo;
    try {
      buildInfo = JSON.parse(result.stdout || "");
    } catch (error) {
      throw new Error(`installed binary emitted invalid build-info JSON: ${error.message}`);
    }
    exactObject(
      buildInfo,
      ["project", "schema_version", "source_dirty", "source_revision", "version"],
      "installed binary build-info",
    );
    if (
      buildInfo.schema_version !== 1 ||
      buildInfo.project !== "git-slop" ||
      buildInfo.version !== version ||
      buildInfo.source_revision !== revision ||
      buildInfo.source_dirty !== false
    ) {
      throw new Error("installed binary build identity does not match the verified release manifest");
    }
  }


  return { verifyInstalledBuildInfo, verifyInstalledVersion };
}
