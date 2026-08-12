import {
  chmodSync,
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, isAbsolute, join } from "node:path";

export function createToolCache({
  exactObject,
  maximumArchiveBytes,
  sha256,
  verifyInstalledBuildInfo,
  verifyInstalledVersion,
}) {
  function toolCacheDirectory(version, target, revision, manifestSha256) {
    const root = (process.env.RUNNER_TOOL_CACHE || "").trim();
    if (!root || !isAbsolute(root)) return null;
    return join(root, "git-slop", version, target, revision, manifestSha256);
  }

  function cachedBinary(cacheDirectory, executableName, expected) {
    if (!cacheDirectory || !existsSync(cacheDirectory)) return null;
    try {
      const binaryPath = join(cacheDirectory, executableName);
      const metadataPath = join(cacheDirectory, "cache-metadata.json");
      if (
        !existsSync(binaryPath) ||
        !existsSync(metadataPath) ||
        !lstatSync(binaryPath).isFile() ||
        lstatSync(binaryPath).isSymbolicLink() ||
        !lstatSync(metadataPath).isFile() ||
        lstatSync(metadataPath).isSymbolicLink()
      ) {
        throw new Error("cache entries must be regular files");
      }
      const metadata = JSON.parse(readFileSync(metadataPath, "utf8"));
      exactObject(
        metadata,
        [
          "archive_sha256",
          "binary_sha256",
          "crate_sha256",
          "manifest_sha256",
          "revision",
          "schema_version",
          "target",
          "version",
        ],
        "tool cache metadata",
      );
      for (const [key, value] of Object.entries(expected)) {
        if (metadata[key] !== value) throw new Error(`tool cache ${key} mismatch`);
      }
      const bytes = readFileSync(binaryPath);
      if (bytes.length === 0 || bytes.length > maximumArchiveBytes || sha256(bytes) !== metadata.binary_sha256) {
        throw new Error("tool cache binary digest mismatch");
      }
      if (process.platform !== "win32") chmodSync(binaryPath, 0o755);
      verifyInstalledVersion(binaryPath, expected.version);
      verifyInstalledBuildInfo(
        binaryPath,
        expected.version,
        expected.revision,
        expected.target,
        metadata.crate_sha256,
      );
      return { binaryPath, crateSha256: metadata.crate_sha256 };
    } catch (error) {
      console.warn(`Discarding invalid Git Slop tool cache entry: ${error.message}`);
      rmSync(cacheDirectory, { recursive: true, force: true });
      return null;
    }
  }

  function populateToolCache(cacheDirectory, executableName, binaryPath, metadata) {
    if (!cacheDirectory) return binaryPath;
    const parent = dirname(cacheDirectory);
    mkdirSync(parent, { recursive: true, mode: 0o700 });
    const staging = mkdtempSync(join(parent, ".git-slop-cache-"));
    const cachedPath = join(staging, executableName);
    copyFileSync(binaryPath, cachedPath);
    if (process.platform !== "win32") chmodSync(cachedPath, 0o755);
    writeFileSync(
      join(staging, "cache-metadata.json"),
      `${JSON.stringify({ ...metadata, binary_sha256: sha256(readFileSync(cachedPath)) })}\n`,
      { mode: 0o600 },
    );
    try {
      renameSync(staging, cacheDirectory);
    } catch (error) {
      rmSync(staging, { recursive: true, force: true });
      if (!existsSync(cacheDirectory)) throw error;
    }
    return join(cacheDirectory, executableName);
  }


  return { cachedBinary, populateToolCache, toolCacheDirectory };
}
