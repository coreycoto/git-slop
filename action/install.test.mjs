import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const actionDirectory = dirname(fileURLToPath(import.meta.url));
const installer = join(actionDirectory, "install.mjs");

function targetTriple() {
  const target = {
    "linux:x64": "x86_64-unknown-linux-gnu",
    "linux:arm64": "aarch64-unknown-linux-gnu",
    "darwin:arm64": "aarch64-apple-darwin",
  }[`${process.platform}:${process.arch}`];
  if (!target) {
    throw new Error(`test does not support ${process.platform}:${process.arch}`);
  }
  return target;
}

function outputs(path) {
  const lines = readFileSync(path, "utf8").split(/\r?\n/u);
  const result = {};
  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(/^([^<]+)<<(.+)$/u);
    if (!match) {
      continue;
    }
    const [, name, delimiter] = match;
    const values = [];
    index += 1;
    while (index < lines.length && lines[index] !== delimiter) {
      values.push(lines[index]);
      index += 1;
    }
    result[name] = values.join("\n");
  }
  return result;
}

function runNode(script, environment) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [script], {
      env: { ...process.env, ...environment },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("close", (status) => resolve({ status, stdout, stderr }));
  });
}

test(
  "installer downloads the exact target archive and verifies its checksum and version",
  {
    skip:
      process.platform === "win32" ||
      (process.platform === "darwin" && process.arch === "x64"),
  },
  async () => {
    const root = mkdtempSync(join(tmpdir(), "git-slop-installer-test-"));
    const version = "0.9.0";
    const tag = `v${version}`;
    const target = targetTriple();
    const assetName = `git-slop-${tag}-${target}.tar.gz`;
    const stageName = `git-slop-${tag}-${target}`;
    const stage = join(root, "stage", stageName);
    const archive = join(root, assetName);
    const output = join(root, "github-output.txt");
    const githubPath = join(root, "github-path.txt");
    mkdirSync(stage, { recursive: true });
    const binary = join(stage, "git-slop");
    writeFileSync(
      binary,
      `#!/usr/bin/env node\nprocess.stdout.write("git-slop ${version}\\n");\n`,
      "utf8",
    );
    chmodSync(binary, 0o755);
    execFileSync("tar", ["-C", join(root, "stage"), "-czf", archive, stageName]);
    const archiveBytes = readFileSync(archive);
    const digest = createHash("sha256").update(archiveBytes).digest("hex");
    const checksumBytes = Buffer.from(`${digest}  ${assetName}\n`, "utf8");

    let apiRoot;
    const server = createServer((request, response) => {
      if (request.url === `/repos/example/git-slop/releases/tags/${tag}`) {
        response.setHeader("content-type", "application/json");
        response.end(
          JSON.stringify({
            assets: [
              {
                name: assetName,
                size: archiveBytes.length,
                url: `${apiRoot}/assets/archive`,
                browser_download_url: `${apiRoot}/downloads/${assetName}`,
              },
              {
                name: "SHA256SUMS",
                size: checksumBytes.length,
                url: `${apiRoot}/assets/checksums`,
                browser_download_url: `${apiRoot}/downloads/SHA256SUMS`,
              },
            ],
          }),
        );
      } else if (request.url === "/assets/archive") {
        response.end(archiveBytes);
      } else if (request.url === "/assets/checksums") {
        response.end(checksumBytes);
      } else {
        response.statusCode = 404;
        response.end("not found");
      }
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    apiRoot = `http://127.0.0.1:${address.port}`;

    try {
      const installed = await runNode(installer, {
        GITHUB_API_URL: apiRoot,
        GITHUB_OUTPUT: output,
        GITHUB_PATH: githubPath,
        GIT_SLOP_ACTION_VERSION: version,
        GIT_SLOP_RELEASE_REPOSITORY: "example/git-slop",
        RUNNER_TEMP: root,
      });
      assert.equal(installed.status, 0, installed.stderr);
      const actual = outputs(output);
      assert.equal(actual.version, version);
      assert.equal(actual.target, target);
      assert.equal(actual.asset, assetName);
      assert.equal(actual.sha256, digest);
      assert.ok(existsSync(actual["binary-path"]));
      assert.equal(readFileSync(githubPath, "utf8").trim(), dirname(actual["binary-path"]));
    } finally {
      await new Promise((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      );
    }
  },
);
