import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { basename, resolve } from "node:path";
import { readdirSync, readFileSync } from "node:fs";

const schemaDirectory = resolve(process.argv[2] ?? "../../schemas");
const binary = process.argv[3] ? resolve(process.argv[3]) : null;
const parse = (path) => JSON.parse(readFileSync(path, "utf8"));
const index = parse(resolve(schemaDirectory, "index.json"));

assert.equal(index.schema_version, 1, "schema index version must be 1");
assert.equal(typeof index.categories, "object", "schema categories are required");
assert.ok(Array.isArray(index.contracts), "schema contracts must be an array");

const expectedKeys = ["category", "file", "lifecycle", "name", "runtime_command"];
const files = readdirSync(schemaDirectory)
  .filter((file) => file.endsWith(".json") && file !== "index.json")
  .sort();
const indexedFiles = index.contracts.map((contract) => contract.file);
assert.deepEqual([...indexedFiles].sort(), files, "every packaged schema must be routed exactly once");
assert.equal(new Set(indexedFiles).size, indexedFiles.length, "schema files must be unique");

const runtimeCommands = [];
for (const contract of index.contracts) {
  assert.deepEqual(Object.keys(contract).sort(), expectedKeys, `unexpected route keys for ${contract.name}`);
  assert.equal(contract.name, contract.file.replace(/\.json$/, ""), `route name drift for ${contract.file}`);
  assert.ok(index.categories[contract.category], `unknown category for ${contract.file}`);
  assert.ok(["current", "compatibility", "historical"].includes(contract.lifecycle), `invalid lifecycle for ${contract.file}`);

  const schema = parse(resolve(schemaDirectory, contract.file));
  assert.equal(schema.$schema, "https://json-schema.org/draft/2020-12/schema", `${contract.file} must use draft 2020-12`);
  assert.equal(basename(new URL(schema.$id).pathname), contract.file, `${contract.file} $id basename drifted`);
  assert.equal(typeof schema.title, "string", `${contract.file} must have a title`);
  assert.ok(
    schema.type !== undefined || schema.oneOf !== undefined || schema.anyOf !== undefined || schema.allOf !== undefined,
    `${contract.file} must declare a root validation shape`,
  );

  if (contract.runtime_command !== null) {
    runtimeCommands.push(contract.runtime_command);
    if (binary) {
      const generated = JSON.parse(execFileSync(binary, ["schema", contract.runtime_command], { encoding: "utf8" }));
      assert.deepEqual(generated, schema, `runtime schema ${contract.runtime_command} drifted from ${contract.file}`);
    }
  } else {
    assert.equal(contract.lifecycle, "historical", `${contract.file} lacks a runtime route but is not historical`);
  }
}

assert.equal(new Set(runtimeCommands).size, runtimeCommands.length, "runtime schema commands must be unique");
console.log(`Validated ${index.contracts.length} routed schemas (${runtimeCommands.length} runtime, ${index.contracts.length - runtimeCommands.length} historical).`);
