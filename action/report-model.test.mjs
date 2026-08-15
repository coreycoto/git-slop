import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(
  new URL("../src/cli/html/report-model.js", import.meta.url),
  "utf8",
);
const context = { console };
context.globalThis = context;
vm.runInNewContext(source, context, { filename: "report-model.js" });
const model = context.GitSlopReportModel;

test("decision-first landing chooses the strongest non-empty surface", () => {
  assert.equal(model.defaultView({ policy: [{}], queue: [{}], health: [{}] }), "policy");
  assert.equal(model.defaultView({ policy: [], queue: [{}], health: [{}] }), "queue");
  assert.equal(model.defaultView({ policy: [], queue: [], health: [{}] }), "health");
  assert.equal(model.defaultView({ policy: [], queue: [], health: [] }), "files");
});

test("context maintenance and review severity filters remain independent", () => {
  const records = [
    { path: "src/a.rs", context_band: "critical", slop_band: "high", severity: "notice" },
    { path: "src/b.rs", context_band: "critical", slop_band: "low", severity: "notice" },
    { path: "src/c.rs", context_band: "healthy", slop_band: "high", severity: "warning" },
  ];
  const filtered = model.filterRecords(records, "src", [
    { key: "context_band", value: "critical" },
    { key: "slop_band", value: "high" },
    { key: "severity", value: "notice" },
  ]);
  assert.deepEqual(filtered.map((record) => record.path), ["src/a.rs"]);
});

test("pagination clamps direct page jumps and preserves bounded page sizes", () => {
  const records = Array.from({ length: 61 }, (_, index) => index);
  const result = model.paginate(records, 99, 25);
  assert.equal(result.page, 2);
  assert.equal(result.pageCount, 3);
  assert.deepEqual(Array.from(result.visible), records.slice(50));
});

test("machine evidence is rendered as human language", () => {
  assert.equal(model.humanizeCode("critical_token_cost"), "Context budget exceeded");
  assert.equal(model.humanizeCode("mapping_confidence_low"), "Test mapping confidence is low");
  assert.equal(model.humanizeCode("temporal_coupling_edge"), "Temporal Coupling Edge");
});
