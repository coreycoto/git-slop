import { writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

function appendFileCommand(target, name, value) {
  if (!target) return;
  const digest = createHash("sha256").update(`${name}\0${value}`).digest("hex").slice(0, 24);
  let delimiter = `git_slop_${name}_${digest}`;
  while (String(value).split(/\r?\n/u).includes(delimiter)) delimiter += "_x";
  writeFileSync(target, `${name}<<${delimiter}\n${value}\n${delimiter}\n`, { flag: "a" });
}

export function setOutput(name, value) {
  appendFileCommand(process.env.GITHUB_OUTPUT, name, String(value));
}

export function boundedInteger(name, fallback, minimum, maximum) {
  const raw = (process.env[name] || String(fallback)).trim();
  if (!/^\d+$/u.test(raw)) {
    throw new Error(`${name} must be an integer, received ${JSON.stringify(raw)}`);
  }
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`${name} must be from ${minimum} through ${maximum}, received ${raw}`);
  }
  return value;
}

export function enumValue(name, fallback, allowed) {
  const raw = (process.env[name] || fallback).trim().toLowerCase();
  if (!allowed.includes(raw)) {
    throw new Error(`${name} must be one of ${allowed.join(", ")}, received ${JSON.stringify(raw)}`);
  }
  return raw;
}

export function run(command, args, cwd, stdio = "inherit") {
  const result = spawnSync(command, args, { cwd, encoding: "utf8", stdio });
  if (result.error) return { status: 2, stderr: result.error.message, stdout: "" };
  return {
    status: result.status ?? 2,
    stderr: result.stderr || "",
    stdout: result.stdout || "",
  };
}

export function safeLogText(value) {
  return String(value).replace(/[\r\n\t\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/gu, (character) => {
    if (character === "\r") return "\\r";
    if (character === "\n") return "\\n";
    if (character === "\t") return "\\t";
    return `\\u{${character.codePointAt(0).toString(16)}}`;
  });
}
