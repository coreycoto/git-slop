((root) => {
  "use strict";

  const reasonLabels = {
    critical_token_cost: "Context budget exceeded",
    high_token_cost: "Approaching the context budget",
    warning_token_cost: "Approaching the context budget (legacy)",
    high_slop_score: "High maintenance pressure",
    high_churn: "High recent churn",
    high_revision_frequency: "Frequently revised",
    high_relative_churn: "High churn relative to file size",
    old_file: "Older file",
    old_and_volatile: "Older file with sustained churn",
    missing_test_evidence: "No nearby test evidence",
    weak_test_mapping: "Weak source-to-test mapping evidence",
    low_test_cochange_evidence: "Low source-and-test co-change evidence",
    mapping_confidence_low: "Test mapping confidence is low",
    evidence_found: "Verification evidence found",
    no_evidence: "No verification evidence found",
  };

  function humanizeCode(value) {
    const text = String(value ?? "");
    if (reasonLabels[text]) return reasonLabels[text];
    return text
      .replaceAll("_", " ")
      .replaceAll("-", " ")
      .replace(/\b\w/g, (letter) => letter.toLocaleUpperCase());
  }

  function defaultView(recordsByView) {
    if (recordsByView.policy?.length) return "policy";
    if (recordsByView.queue?.length) return "queue";
    if (recordsByView.health?.length) return "health";
    if (recordsByView.observations?.length) return "observations";
    return "files";
  }

  function filterRecords(records, query, filters) {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return records.filter((record) => {
      const haystack = [
        record.path,
        record.id,
        record.title,
        record.message,
        record.kind,
        record.source_path,
        record.target_path,
        ...(record.member_paths ?? []),
        ...(record.reason_codes ?? []),
      ]
        .join(" ")
        .toLocaleLowerCase();
      return (
        (!normalizedQuery || haystack.includes(normalizedQuery)) &&
        filters.every(({ key, value }) => !value || String(record[key] ?? "") === value)
      );
    });
  }

  function paginate(records, requestedPage, pageSize) {
    const pageCount = Math.max(1, Math.ceil(records.length / pageSize));
    const page = Math.max(0, Math.min(pageCount - 1, requestedPage));
    return {
      page,
      pageCount,
      visible: records.slice(page * pageSize, (page + 1) * pageSize),
    };
  }

  root.GitSlopReportModel = Object.freeze({
    defaultView,
    filterRecords,
    humanizeCode,
    paginate,
  });
})(globalThis);
