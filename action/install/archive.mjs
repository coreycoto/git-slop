import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

export function createArchiveTools({
  apiRoot,
  completionFileNames,
  fetchJsonRequired,
  maximumArchiveBytes,
  maximumReleaseMetadataBytes,
  maximumTagPeelDepth,
  maximumInventoryBytes,
  maximumCrateMetadataBytes,
  sha256,
  versionedSchemaName,
}) {
  function validateArchiveFormat(bytes, archive) {
    if (!Buffer.isBuffer(bytes) || bytes.length < 4) {
      throw new Error(`release archive is too small to be a valid ${archive} file`);
    }
    if (archive === "tar.gz") {
      if (bytes[0] !== 0x1f || bytes[1] !== 0x8b || bytes[2] !== 0x08) {
        throw new Error("release archive does not use the required tar.gz format");
      }
      return;
    }
    if (archive === "zip") {
      if (bytes[0] !== 0x50 || bytes[1] !== 0x4b || bytes[2] !== 0x03 || bytes[3] !== 0x04) {
        throw new Error("release archive does not use the required ZIP format");
      }
      return;
    }
    throw new Error(`unsupported release archive format ${archive}`);
  }

  function archiveTarExecutable() {
    if (process.platform !== "win32") {
      return "tar";
    }

    const systemRoot =
      (process.env.SystemRoot || "").trim() || (process.env.WINDIR || "").trim();
    if (!systemRoot || !isAbsolute(systemRoot)) {
      throw new Error("Windows SystemRoot must be an absolute path");
    }
    const executable = join(systemRoot, "System32", "tar.exe");
    if (!existsSync(executable)) {
      throw new Error(`Windows built-in tar.exe is unavailable at ${executable}`);
    }
    return executable;
  }

  function runTar(args, options = {}) {
    const result = spawnSync(archiveTarExecutable(), args, {
      cwd: options.cwd,
      encoding: Object.hasOwn(options, "encoding") ? options.encoding : "utf8",
      maxBuffer: options.maxBuffer ?? maximumInventoryBytes,
      stdio: ["ignore", "pipe", "pipe"],
    });
    if (result.error) {
      throw new Error(`archive inspection failed: ${result.error.message}`);
    }
    if (result.status !== 0) {
      const stderr = Buffer.isBuffer(result.stderr)
        ? result.stderr.toString("utf8")
        : result.stderr || "";
      throw new Error(`archive inspection failed: ${stderr.trim()}`);
    }
    return result.stdout;
  }

  function runArchiveTar(archivePath, operationArguments, memberArguments = [], options = {}) {
    const absoluteArchivePath = resolve(archivePath);
    return runTar(
      [...operationArguments, "-f", basename(absoluteArchivePath), ...memberArguments],
      {
        ...options,
        cwd: dirname(absoluteArchivePath),
      },
    );
  }

  function validateArchiveMember(member, rootName) {
    if (
      !member ||
      member.includes("\\") ||
      member.startsWith("/") ||
      member.startsWith("~") ||
      /^[A-Za-z]:/u.test(member)
    ) {
      throw new Error(`archive contains an unsafe member name: ${JSON.stringify(member)}`);
    }
    const components = member.replace(/\/$/u, "").split("/");
    if (components.some((component) => component === "" || component === "." || component === "..")) {
      throw new Error(`archive contains an unsafe member name: ${JSON.stringify(member)}`);
    }
    if (components[0] !== rootName) {
      throw new Error(`archive member escapes the expected root ${rootName}: ${member}`);
    }
  }

  function validateCrateInventory(archivePath, rootName) {
    const inventoryText = runArchiveTar(archivePath, ["-t"]);
    const inventory = inventoryText.split(/\r?\n/u).filter(Boolean);
    if (inventory.length === 0 || inventory.length > 4096) {
      throw new Error("canonical crates.io package has an invalid archive inventory size");
    }
    const seen = new Set();
    for (const member of inventory) {
      validateArchiveMember(member, rootName);
      if (seen.has(member)) {
        throw new Error(`canonical crates.io package contains a duplicate member: ${member}`);
      }
      seen.add(member);
    }
    for (const member of [
      `${rootName}/Cargo.toml`,
      `${rootName}/.cargo_vcs_info.json`,
      `${rootName}/src/main.rs`,
    ]) {
      if (!seen.has(member)) {
        throw new Error(`canonical crates.io package is missing required member ${member}`);
      }
    }
  }

  function verifyCrateVcsMetadata(archivePath, rootName, revision) {
    const member = `${rootName}/.cargo_vcs_info.json`;
    const bytes = runArchiveTar(
      archivePath,
      ["-xO"],
      [member],
      {
        encoding: null,
        maxBuffer: maximumCrateMetadataBytes,
      },
    );
    if (
      !Buffer.isBuffer(bytes) ||
      bytes.length === 0 ||
      bytes.length > maximumCrateMetadataBytes
    ) {
      throw new Error("canonical crates.io package has invalid VCS metadata size");
    }
    let metadata;
    try {
      metadata = JSON.parse(bytes.toString("utf8"));
    } catch (error) {
      throw new Error(`canonical crates.io package has invalid VCS metadata: ${error.message}`);
    }
    if (
      metadata === null ||
      typeof metadata !== "object" ||
      Array.isArray(metadata) ||
      metadata.git === null ||
      typeof metadata.git !== "object" ||
      Array.isArray(metadata.git) ||
      metadata.git.sha1 !== revision ||
      (metadata.git.dirty !== undefined && metadata.git.dirty !== false) ||
      metadata.path_in_vcs !== ""
    ) {
      throw new Error(
        "canonical crates.io package VCS metadata does not match the clean release revision",
      );
    }
  }

  function verifyCanonicalCrate(cratePath, crateBytes, version, expectedSha256, revision) {
    validateArchiveFormat(crateBytes, "tar.gz");
    const actualSha256 = sha256(crateBytes);
    if (actualSha256 !== expectedSha256) {
      throw new Error(
        `canonical crates.io package SHA-256 mismatch: expected ${expectedSha256}, received ${actualSha256}`,
      );
    }
    const rootName = `git-slop-${version}`;
    validateCrateInventory(cratePath, rootName);
    verifyCrateVcsMetadata(cratePath, rootName, revision);
    return actualSha256;
  }

  async function exactTagRevision(repository, tag) {
    const encodedTag = encodeURIComponent(tag);
    const reference = await fetchJsonRequired(
      `${apiRoot}/repos/${repository}/git/ref/tags/${encodedTag}`,
      maximumReleaseMetadataBytes,
      `release tag reference for ${tag}`,
    );
    if (
      reference === null ||
      typeof reference !== "object" ||
      Array.isArray(reference) ||
      reference.ref !== `refs/tags/${tag}` ||
      reference.object === null ||
      typeof reference.object !== "object" ||
      Array.isArray(reference.object) ||
      !["commit", "tag"].includes(reference.object.type) ||
      !/^[a-f0-9]{40}$/u.test(reference.object.sha || "")
    ) {
      throw new Error(`release tag ${tag} has invalid exact-reference metadata`);
    }

    let objectType = reference.object.type;
    let objectSha = reference.object.sha;
    const seen = new Set();
    for (let depth = 0; depth <= maximumTagPeelDepth; depth += 1) {
      if (objectType === "commit") {
        return objectSha;
      }
      if (depth === maximumTagPeelDepth || seen.has(objectSha)) {
        throw new Error(`release tag ${tag} exceeds the safe annotated-tag peel limit`);
      }
      seen.add(objectSha);
      const annotated = await fetchJsonRequired(
        `${apiRoot}/repos/${repository}/git/tags/${objectSha}`,
        maximumReleaseMetadataBytes,
        `annotated release tag object ${objectSha}`,
      );
      if (
        annotated === null ||
        typeof annotated !== "object" ||
        Array.isArray(annotated) ||
        annotated.sha !== objectSha ||
        (depth === 0 && annotated.tag !== tag) ||
        annotated.object === null ||
        typeof annotated.object !== "object" ||
        Array.isArray(annotated.object) ||
        !["commit", "tag"].includes(annotated.object.type) ||
        !/^[a-f0-9]{40}$/u.test(annotated.object.sha || "")
      ) {
        throw new Error(`release tag ${tag} has invalid annotated-tag metadata`);
      }
      objectType = annotated.object.type;
      objectSha = annotated.object.sha;
    }
    throw new Error(`release tag ${tag} could not be resolved to a commit`);
  }

  function materializeArchive(archivePath, installRoot, rootName, executableName) {
    const expectedFiles = ["LICENSE", "README.md", "man/git-slop.1", executableName];
    const rootMember = `${rootName}/`;
    const manMember = `${rootName}/man/`;
    const completionsMember = `${rootName}/completions/`;
    const schemasMember = `${rootName}/schemas/`;
    const expectedFileMembers = expectedFiles.map((name) => `${rootName}/${name}`);
    const expectedCompletionMembers = completionFileNames.map(
      (name) => `${completionsMember}${name}`,
    );
    const allowedMembers = new Set([
      rootMember,
      manMember,
      completionsMember,
      schemasMember,
      ...expectedFileMembers,
      ...expectedCompletionMembers,
    ]);
    const inventoryText = runArchiveTar(archivePath, ["-t"]);
    const inventory = inventoryText.split(/\r?\n/u).filter(Boolean);
    if (inventory.length === 0 || inventory.length > 4096) {
      throw new Error("archive inventory does not match the exact Git Slop release layout");
    }
    const actualMembers = new Set();
    const schemaMembers = new Set();
    for (const member of inventory) {
      validateArchiveMember(member, rootName);
      if (actualMembers.has(member)) {
        throw new Error(`archive contains a duplicate member: ${member}`);
      }
      const schemaName = member.startsWith(schemasMember)
        ? member.slice(schemasMember.length)
        : "";
      if (!allowedMembers.has(member) && !versionedSchemaName.test(schemaName)) {
        throw new Error("archive inventory does not match the exact Git Slop release layout");
      }
      if (schemaName) {
        schemaMembers.add(member);
      }
      actualMembers.add(member);
    }
    if (
      [...expectedFileMembers, ...expectedCompletionMembers].some(
        (member) => !actualMembers.has(member),
      ) || schemaMembers.size === 0
    ) {
      throw new Error("archive inventory does not match the exact Git Slop release layout");
    }

    const payloadRoot = join(installRoot, "payload");
    mkdirSync(payloadRoot, { mode: 0o700 });
    for (const name of expectedFiles) {
      const member = `${rootName}/${name}`;
      const maximumBytes = name === executableName ? maximumArchiveBytes : 4 * 1024 * 1024;
      const bytes = runArchiveTar(
        archivePath,
        ["-xO"],
        [member],
        {
          encoding: null,
          maxBuffer: maximumBytes,
        },
      );
      if (!Buffer.isBuffer(bytes) || bytes.length === 0 || bytes.length > maximumBytes) {
        throw new Error(`archive member ${member} has an invalid size`);
      }
      const target = join(payloadRoot, name);
      mkdirSync(dirname(target), { recursive: true, mode: 0o700 });
      writeFileSync(target, bytes, {
        mode: name === executableName ? 0o700 : 0o600,
      });
    }
    return join(payloadRoot, executableName);
  }


  return {
    archiveTarExecutable,
    exactTagRevision,
    materializeArchive,
    validateArchiveFormat,
    verifyCanonicalCrate,
  };
}
