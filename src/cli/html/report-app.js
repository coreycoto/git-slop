(() => {
  "use strict";

  const report = JSON.parse(document.getElementById("report").textContent);
  const model = globalThis.GitSlopReportModel;
  const params = new URLSearchParams(window.location.search);
  const viewNames = {
    policy: "Policy failures",
    queue: "Interventions",
    observations: "Observations",
    health: "Advisory health findings",
    files: "Files",
    folders: "Folders",
    relationships: "Relationships",
    clusters: "Clusters",
  };
  const recordsByView = {
    policy: report.policy_failures ?? [],
    queue: report.action_queue ?? [],
    observations: report.observation_feed ?? [],
    health: report.health?.findings ?? [],
    files: report.files ?? [],
    folders: report.folders ?? [],
    relationships: report.organization?.relationships ?? [],
    clusters: report.organization?.clusters ?? [],
  };
  const reviewColumns = [
    ["path", "Path"], ["__rank", "Rank"], ["severity", "Severity"],
    ["context_band", "Context"], ["slop_band", "Maintenance"],
    ["slop_score", "Score"], ["reason_codes", "Reasons"],
    ["next_action", "Next action"],
  ];
  const columns = {
    policy: [
      ["path", "Path"], ["context_band", "Context"],
      ["slop_band", "Maintenance"], ["slop_score", "Score"],
      ["tokens", "Tokens"], ["classification", "Classification"],
    ],
    queue: reviewColumns,
    observations: reviewColumns,
    health: [
      ["path", "Path"], ["severity", "Severity"],
      ["title", "Finding"], ["message", "Message"],
    ],
    files: [
      ["path", "Path"], ["slop_score", "Score"], ["tokens", "Tokens"],
      ["context_band", "Context"], ["slop_band", "Maintenance"],
      ["language", "Language"], ["profile", "Profile"],
      ["classification", "Classification"],
    ],
    folders: [
      ["path", "Folder"], ["slop_score", "Score"], ["tokens", "Tokens"],
      ["context_band", "Context"], ["slop_band", "Maintenance"],
      ["classification", "Classification"],
    ],
    relationships: [
      ["id", "Relationship"], ["kind", "Kind"], ["source_path", "Source"],
      ["target_path", "Target"], ["confidence", "Confidence"],
      ["support_count", "Support"], ["evidence_score", "Evidence"],
    ],
    clusters: [
      ["id", "Cluster"], ["kind", "Kind"], ["member_count", "Count"],
      ["member_paths", "Members"], ["evidence_score", "Evidence"],
    ],
  };
  const sortDefaults = {
    policy: { key: "slop_score", ascending: false },
    queue: { key: "__rank", ascending: true },
    observations: { key: "__rank", ascending: true },
    health: { key: "severity", ascending: false },
    files: { key: "slop_score", ascending: false },
    folders: { key: "slop_score", ascending: false },
    relationships: { key: "evidence_score", ascending: false },
    clusters: { key: "evidence_score", ascending: false },
  };
  const sortState = Object.fromEntries(
    Object.entries(sortDefaults).map(([key, value]) => [key, { ...value }]),
  );
  const severityOrder = {
    unknown: 0, notice: 1, low: 1, compact: 1, warning: 2, moderate: 2,
    healthy: 2, high: 3, error: 4, critical: 5, budget_exceeded: 5,
  };

  Object.values(recordsByView).forEach((records) => {
    records.forEach((record, index) => {
      Object.defineProperty(record, "__rank", {
        configurable: false, enumerable: false, value: index + 1,
      });
    });
  });

  const elements = {
    classification: document.getElementById("classification"),
    closeDetail: document.getElementById("close-detail"),
    contextBand: document.getElementById("context-band"),
    count: document.getElementById("count"),
    detailCommands: document.getElementById("detail-commands"),
    detailEvidence: document.getElementById("detail-evidence"),
    detailMetrics: document.getElementById("detail-metrics"),
    detailPanel: document.getElementById("detail-panel"),
    detailRaw: document.getElementById("detail-raw"),
    detailSummary: document.getElementById("detail-summary"),
    detailTitle: document.getElementById("detail-title"),
    first: document.getElementById("first"),
    headers: document.getElementById("headers"),
    language: document.getElementById("language"),
    last: document.getElementById("last"),
    next: document.getElementById("next"),
    overviewGrid: document.getElementById("overview-grid"),
    pageNumber: document.getElementById("page-number"),
    pageSize: document.getElementById("page-size"),
    previous: document.getElementById("previous"),
    profile: document.getElementById("profile"),
    query: document.getElementById("query"),
    resetFilters: document.getElementById("reset-filters"),
    rows: document.getElementById("rows"),
    selectionStatus: document.getElementById("selection-status"),
    severity: document.getElementById("severity"),
    slopBand: document.getElementById("slop-band"),
    sortState: document.getElementById("sort-state"),
    truncation: document.getElementById("truncation"),
  };
  const defaultView = model.defaultView(recordsByView);
  let view = Object.hasOwn(viewNames, params.get("view")) ? params.get("view") : defaultView;
  const requestedPage = Number.parseInt(params.get("page") ?? "1", 10) || 1;
  let page = Math.max(0, requestedPage - 1);
  let pageSize = [25, 100, 250].includes(Number(params.get("size")))
    ? Number(params.get("size")) : 100;
  let pageCount = 1;
  let selectedIdentity = "";
  let selectedButton = null;
  let pendingRecord = params.get("record") ?? "";
  let restoreInitialFilters = true;

  if (columns[view].some(([key]) => key === params.get("sort"))) {
    sortState[view] = { key: params.get("sort"), ascending: params.get("dir") === "asc" };
  }
  elements.pageSize.value = String(pageSize);
  elements.query.value = params.get("q") ?? "";
  document.getElementById("descriptor").textContent = `${report.repo?.repo_name ?? "repository"} · ${report.generated_at ?? "unknown time"}`;
  document.getElementById("schema-badge").textContent = `Schema ${report.schema_version ?? "unknown"}`;
  const freshness = report.freshness;
  document.getElementById("freshness").textContent = freshness && !freshness.current
    ? `This is a report snapshot and does not match the current repository (${(freshness.reasons ?? []).map((reason) => reason.code ?? reason).join(", ") || freshness.status || "stale"}). Regenerate with git slop find before acting.`
    : "";
  document.getElementById("evidence-summary").textContent = JSON.stringify({
    config_digests: report.config_digests,
    completeness: report.evidence_completeness,
    collections: report.collection_metadata,
    embedded: report.embedded_evidence,
    freshness: report.freshness,
    source_report: report.source_report,
  }, null, 2);

  function escapeHtml(value) {
    return String(value ?? "").replace(/[&<>"']/g, (character) => ({
      "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
    })[character]);
  }

  function humanizeCode(value) {
    return model.humanizeCode(value);
  }

  function displayValue(value, key = "") {
    if (Array.isArray(value)) {
      return value.map((item) => key === "reason_codes" ? humanizeCode(item) : item).join(", ");
    }
    if (value === true) return "Yes";
    if (value === false) return "No";
    if (value === null || value === undefined || value === "") return "—";
    if (typeof value === "number" && !Number.isInteger(value)) {
      return value.toLocaleString(undefined, { maximumFractionDigits: 3 });
    }
    if (typeof value === "object") return JSON.stringify(value);
    if (["severity", "slop_band", "context_band", "classification", "evidence_status", "kind"].includes(key)) {
      return humanizeCode(value);
    }
    return String(value);
  }

  function recordIdentity(recordView, record) {
    if (recordView === "relationships" || recordView === "clusters") {
      return `${recordView}:${record.id ?? "unknown"}`;
    }
    if (recordView === "health") {
      return `health:${record.id ?? `${record.path ?? "repository"}:${record.title ?? record.message ?? record.severity ?? "finding"}`}`;
    }
    return `${recordView}:${record.id ?? record.path ?? record.__rank}`;
  }

  function stableDomId(identity) {
    let hash = 2166136261;
    for (let index = 0; index < identity.length; index += 1) {
      hash ^= identity.charCodeAt(index);
      hash = Math.imul(hash, 16777619);
    }
    return `record-${(hash >>> 0).toString(16).padStart(8, "0")}`;
  }

  function shellQuote(value) {
    return `'${String(value).replaceAll("'", `'\\''`)}'`;
  }

  function viewMetadata() {
    const keys = {
      policy: "policy_failures", queue: "action_queue",
      observations: "observation_feed", health: "health",
    };
    const key = keys[view] ?? view;
    return report.embedded_evidence?.view_metadata?.[key] ?? {
      embedded: recordsByView[view].length,
      total: recordsByView[view].length,
      truncated: false,
    };
  }

  function syncUrl() {
    const url = new URL(window.location.href);
    url.search = "";
    const values = {
      view, q: elements.query.value, profile: elements.profile.value,
      classification: elements.classification.value, language: elements.language.value,
      context: elements.contextBand.value, maintenance: elements.slopBand.value,
      severity: elements.severity.value, sort: sortState[view].key,
      dir: sortState[view].ascending ? "asc" : "desc",
      size: pageSize === 100 ? "" : String(pageSize),
      page: page === 0 ? "" : String(page + 1), record: selectedIdentity,
    };
    Object.entries(values).forEach(([key, value]) => {
      if (value !== "") url.searchParams.set(key, value);
    });
    url.hash = selectedIdentity ? stableDomId(selectedIdentity) : "";
    window.history.replaceState(null, "", url);
  }

  function clearSelection({ updateUrl = false, announce = false } = {}) {
    if (selectedIdentity) {
      document.querySelector(`[data-identity="${CSS.escape(selectedIdentity)}"]`)
        ?.closest("tr")?.removeAttribute("data-selected");
    }
    selectedIdentity = "";
    selectedButton = null;
    elements.detailPanel.hidden = true;
    elements.selectionStatus.textContent = announce ? "Selection cleared." : "";
    if (updateUrl) syncUrl();
  }

  function rebuildFilter(select, values, label) {
    const previous = select.value;
    const options = [...new Set(values.filter(Boolean).map(String))]
      .sort((left, right) => left.localeCompare(right));
    select.replaceChildren(new Option(label, ""),
      ...options.map((value) => new Option(humanizeCode(value), value)));
    if (options.includes(previous)) select.value = previous;
  }

  function valueFor(record, key) {
    if (key === "member_count") return record.member_count ?? record.member_paths?.length;
    if (key === "tokens") {
      return record.tokens ?? record.costs?.load?.file_token_count ?? record.costs?.load?.folder_token_count;
    }
    return record[key];
  }

  function pathButton(path) {
    if (!path) return "—";
    return `<button type="button" class="file-link" data-path="${escapeHtml(path)}"><code>${escapeHtml(path)}</code></button>`;
  }

  function renderCell(record, key, column) {
    const value = valueFor(record, key);
    if (view === "relationships" && (key === "source_path" || key === "target_path")) {
      return pathButton(value);
    }
    if (view === "clusters" && key === "member_paths") {
      return (record.member_paths ?? []).map(pathButton).join(", ");
    }
    if (column === 0) {
      const label = value ?? record.path ?? record.id ?? "Record";
      const identity = recordIdentity(view, record);
      return `<button type="button" class="record" data-identity="${escapeHtml(identity)}" aria-controls="detail-panel"><code>${escapeHtml(displayValue(label, key))}</code></button>`;
    }
    return escapeHtml(displayValue(value, key));
  }

  function compareValues(leftRecord, rightRecord) {
    const { key, ascending } = sortState[view];
    let left = valueFor(leftRecord, key);
    let right = valueFor(rightRecord, key);
    if (key === "severity" || key.endsWith("_band")) {
      left = severityOrder[String(left ?? "unknown").toLocaleLowerCase()] ?? 0;
      right = severityOrder[String(right ?? "unknown").toLocaleLowerCase()] ?? 0;
    }
    let compared;
    if (typeof left === "number" && typeof right === "number") compared = left - right;
    else compared = String(left ?? "").localeCompare(String(right ?? ""), undefined, { numeric: true });
    if (compared === 0) {
      compared = recordIdentity(view, leftRecord).localeCompare(recordIdentity(view, rightRecord));
    }
    return compared * (ascending ? 1 : -1);
  }

  function detailContent(record) {
    const path = record.path ?? record.source_path ?? record.member_paths?.[0];
    const title = record.id ?? record.path ?? record.title ?? `${viewNames[view]} record`;
    const summaries = {
      policy: "A configured policy threshold failed for this path.",
      queue: record.next_action ?? "A bounded maintenance candidate that warrants review.",
      observations: record.next_action ?? "An advisory observation that does not request intervention.",
      health: record.message ?? "An advisory repository-health finding.",
      files: "File-level context load and maintenance-pressure evidence.",
      folders: "Folder-level context load and maintenance-pressure evidence.",
      relationships: `${humanizeCode(record.kind ?? "relationship")} evidence connecting ${record.source_path ?? "the source"} and ${record.target_path ?? "the target"}.`,
      clusters: `${humanizeCode(record.kind ?? "cluster")} containing ${record.member_count ?? record.member_paths?.length ?? 0} reported members.`,
    };
    const metricKeys = {
      policy: ["slop_score", "slop_band", "context_band", "classification", "tokens"],
      queue: ["__rank", "severity", "slop_score", "slop_band", "context_band", "evidence_status"],
      observations: ["__rank", "severity", "slop_score", "slop_band", "context_band", "evidence_status"],
      health: ["severity", "path", "title"],
      files: ["slop_score", "slop_band", "context_band", "profile", "classification", "language", "tokens"],
      folders: ["slop_score", "slop_band", "context_band", "classification", "tokens"],
      relationships: ["kind", "evidence_score", "evidence_lower_bound", "confidence", "support_count"],
      clusters: ["kind", "member_count", "evidence_score", "candidate_type"],
    }[view];
    const evidence = {
      "Reasons": [...(record.reason_codes ?? []), ...(record.reasons ?? [])],
      "Related paths": [...(record.member_paths ?? [])],
      "Relationships": [...(record.source_relationship_ids ?? [])],
      "Clusters": [...(record.cluster_ids ?? [])],
    };
    if (view === "relationships") evidence["Related paths"].push(record.source_path, record.target_path);
    const commands = [record.next_command, record.next_action]
      .filter((command) => typeof command === "string" && command.startsWith("git slop "));
    if (path) {
      commands.push(`git slop explain --path ${shellQuote(path)}`);
      commands.push(`git slop plan --path ${shellQuote(path)}`);
    }
    if (view === "relationships" && record.id) {
      commands.unshift(`git slop explain --relationship ${shellQuote(record.id)}`);
      commands.push(`git slop plan --relationship ${shellQuote(record.id)}`);
    }
    if (view === "clusters" && record.id) {
      commands.unshift(`git slop explain --cluster ${shellQuote(record.id)}`);
      commands.push(`git slop plan --cluster ${shellQuote(record.id)}`);
    }
    return {
      commands: [...new Set(commands)], evidence, metricKeys, summary: summaries[view], title,
    };
  }

  function selectRecord(record, button, { focus = true, updateUrl = true } = {}) {
    clearSelection();
    const content = detailContent(record);
    selectedIdentity = recordIdentity(view, record);
    selectedButton = button ?? null;
    button?.closest("tr")?.setAttribute("data-selected", "true");
    elements.detailTitle.textContent = content.title;
    elements.detailSummary.textContent = content.summary;
    elements.detailMetrics.innerHTML = content.metricKeys.map((key) =>
      `<div class="metric"><dt>${escapeHtml(humanizeCode(key))}</dt><dd>${escapeHtml(displayValue(valueFor(record, key), key))}</dd></div>`).join("");
    elements.detailEvidence.innerHTML = Object.entries(content.evidence)
      .filter(([, values]) => values.filter(Boolean).length)
      .map(([label, values]) => `<div class="detail-section"><h3>${escapeHtml(label)}</h3><ul>${values.filter(Boolean).map((value) =>
        label === "Related paths" ? `<li>${pathButton(value)}</li>`
          : `<li>${escapeHtml(label === "Reasons" ? humanizeCode(value) : value)}</li>`).join("")}</ul></div>`)
      .join("") || "<div class=\"detail-section\"><h3>Supporting evidence</h3><p class=\"muted\">No additional evidence was reported.</p></div>";
    elements.detailCommands.innerHTML = content.commands.length
      ? content.commands.map((command) => `<div class="command-row"><code>${escapeHtml(command)}</code><button type="button" data-copy="${escapeHtml(command)}">Copy</button></div>`).join("")
      : "<span class=\"muted\">No path-specific command is available for this record.</span>";
    elements.detailRaw.textContent = JSON.stringify(record, null, 2);
    elements.detailPanel.hidden = false;
    elements.selectionStatus.textContent = `Selected ${content.title}.`;
    if (updateUrl) syncUrl();
    if (focus) elements.detailTitle.focus({ preventScroll: false });
  }

  function renderOverview() {
    const cards = [
      ["policy", "Policy failures", report.overview?.policy_failures ?? recordsByView.policy.length, "Enforced thresholds"],
      ["queue", "Interventions", report.overview?.interventions ?? recordsByView.queue.length, "Review candidates"],
      ["observations", "Observations", report.overview?.observations ?? recordsByView.observations.length, "Advisory signals"],
      ["health", "Health findings", report.overview?.advisory_health_findings ?? recordsByView.health.length, "Advisory diagnostics"],
    ];
    elements.overviewGrid.innerHTML = cards.map(([target, label, count, description]) =>
      `<button type="button" class="overview-card" data-view="${target}" aria-pressed="false"><span>${escapeHtml(label)}</span><strong>${Number(count).toLocaleString()}</strong><small>${escapeHtml(description)}</small></button>`).join("");
  }

  function render() {
    const source = recordsByView[view];
    const activeColumns = columns[view];
    const query = elements.query.value;
    document.querySelectorAll("[data-view]").forEach((button) => {
      button.setAttribute("aria-pressed", String(button.dataset.view === view));
    });
    const filterDefinitions = [
      [elements.profile, "profile", "All profiles", "profile"],
      [elements.classification, "classification", "All classifications", "classification"],
      [elements.language, "language", "All languages", "language"],
      [elements.contextBand, "context_band", "All context bands", "context"],
      [elements.slopBand, "slop_band", "All maintenance bands", "maintenance"],
      [elements.severity, "severity", "All severities", "severity"],
    ];
    filterDefinitions.forEach(([select, key, label]) => {
      rebuildFilter(select, source.map((record) => valueFor(record, key)), label);
      select.disabled = !source.some((record) => valueFor(record, key));
    });
    if (restoreInitialFilters) {
      filterDefinitions.forEach(([select, , , parameter]) => {
        const requested = params.get(parameter) ?? "";
        if ([...select.options].some((option) => option.value === requested)) select.value = requested;
      });
      restoreInitialFilters = false;
    }
    const selected = model.filterRecords(
      source,
      query,
      filterDefinitions.map(([select, key]) => ({ key, value: select.value })),
    ).sort(compareValues);
    let deepLinkRecord = null;
    if (pendingRecord) {
      const index = selected.findIndex((record) => recordIdentity(view, record) === pendingRecord
        || record.path === pendingRecord || record.id === pendingRecord);
      if (index >= 0) {
        page = Math.floor(index / pageSize);
        deepLinkRecord = selected[index];
      }
    }
    const pagination = model.paginate(selected, page, pageSize);
    page = pagination.page;
    pageCount = pagination.pageCount;
    const visible = pagination.visible;
    elements.headers.innerHTML = activeColumns.map(([key, label]) => {
      const active = key === sortState[view].key;
      const direction = active ? (sortState[view].ascending ? "ascending" : "descending") : "none";
      return `<th scope="col" aria-sort="${direction}"><button type="button" data-key="${escapeHtml(key)}">${escapeHtml(label)}</button></th>`;
    }).join("");
    const emptyMessage = source.length === 0
      ? `No ${viewNames[view].toLocaleLowerCase()} were reported. Run git slop find to refresh repository evidence.`
      : `No ${viewNames[view].toLocaleLowerCase()} match the active search and filters. Reset filters to review the complete collection.`;
    elements.rows.innerHTML = visible.length
      ? visible.map((record) => {
        const identity = recordIdentity(view, record);
        return `<tr id="${stableDomId(identity)}">${activeColumns.map((column, index) =>
          `<td>${renderCell(record, column[0], index)}</td>`).join("")}</tr>`;
      }).join("")
      : `<tr><td class="empty-row" colspan="${activeColumns.length}">${escapeHtml(emptyMessage)}</td></tr>`;
    const metadata = viewMetadata();
    elements.count.textContent = `${selected.length.toLocaleString()} filtered · ${metadata.embedded.toLocaleString()} embedded of ${metadata.total.toLocaleString()} total · page ${page + 1} of ${pageCount}`;
    elements.first.disabled = page === 0;
    elements.previous.disabled = page === 0;
    elements.next.disabled = page + 1 >= pageCount;
    elements.last.disabled = page + 1 >= pageCount;
    elements.pageNumber.value = String(page + 1);
    elements.pageNumber.max = String(pageCount);
    const sortLabel = activeColumns.find(([key]) => key === sortState[view].key)?.[1]
      ?? sortState[view].key;
    elements.sortState.textContent = `Sorted by ${sortLabel}, ${sortState[view].ascending ? "ascending" : "descending"}.`;
    elements.truncation.textContent = metadata.truncated
      ? `${viewNames[view]} are truncated in this portable report.${report.source_report
        ? ` Open ${report.source_report} for complete evidence.`
        : " Regenerate from the source JSON to inspect complete evidence."}` : "";
    if (pendingRecord) {
      if (deepLinkRecord) {
        const identity = recordIdentity(view, deepLinkRecord);
        const button = document.querySelector(`.record[data-identity="${CSS.escape(identity)}"]`);
        selectRecord(deepLinkRecord, button, { focus: false, updateUrl: false });
      } else {
        elements.selectionStatus.textContent = `The selected record was not found in the filtered, embedded ${viewNames[view].toLocaleLowerCase()} view.`;
      }
      pendingRecord = "";
    }
    syncUrl();
  }

  function resetFilters() {
    [elements.profile, elements.classification, elements.language, elements.contextBand,
      elements.slopBand, elements.severity].forEach((select) => { select.value = ""; });
  }

  elements.resetFilters.addEventListener("click", () => {
    clearSelection();
    resetFilters();
    elements.query.value = "";
    page = 0;
    render();
    elements.query.focus();
  });

  function openPath(path) {
    clearSelection();
    view = "files";
    page = 0;
    resetFilters();
    elements.query.value = path;
    pendingRecord = `files:${path}`;
    render();
  }

  document.addEventListener("click", (event) => {
    const pathLink = event.target.closest(".file-link");
    if (pathLink) {
      openPath(pathLink.dataset.path);
      return;
    }
    const viewButton = event.target.closest("[data-view]");
    if (viewButton) {
      clearSelection();
      view = viewButton.dataset.view;
      page = 0;
      resetFilters();
      render();
    }
  });
  elements.rows.addEventListener("click", (event) => {
    const button = event.target.closest(".record");
    if (!button) return;
    const record = recordsByView[view].find((candidate) =>
      recordIdentity(view, candidate) === button.dataset.identity);
    if (record) selectRecord(record, button);
  });
  elements.headers.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-key]");
    if (!button) return;
    clearSelection();
    const state = sortState[view];
    if (state.key === button.dataset.key) state.ascending = !state.ascending;
    else sortState[view] = { key: button.dataset.key, ascending: true };
    page = 0;
    render();
  });
  [elements.query, elements.profile, elements.classification, elements.language,
    elements.contextBand, elements.slopBand, elements.severity].forEach((control) => {
    control.addEventListener("input", () => {
      clearSelection();
      page = 0;
      render();
    });
  });
  const goToPage = (target) => {
    clearSelection();
    page = Math.max(0, Math.min(pageCount - 1, target));
    render();
    document.querySelector(".table-shell").focus();
  };
  elements.first.addEventListener("click", () => goToPage(0));
  elements.previous.addEventListener("click", () => goToPage(page - 1));
  elements.next.addEventListener("click", () => goToPage(page + 1));
  elements.last.addEventListener("click", () => goToPage(pageCount - 1));
  elements.pageNumber.addEventListener("change", () =>
    goToPage(Number(elements.pageNumber.value) - 1));
  elements.pageSize.addEventListener("change", () => {
    pageSize = Number(elements.pageSize.value);
    goToPage(0);
  });
  elements.closeDetail.addEventListener("click", () => {
    const restoreFocus = selectedButton;
    clearSelection({ announce: true, updateUrl: true });
    restoreFocus?.focus();
  });
  elements.detailCommands.addEventListener("click", async (event) => {
    const button = event.target.closest("[data-copy]");
    if (!button) return;
    try {
      await navigator.clipboard.writeText(button.dataset.copy);
      button.textContent = "Copied";
    } catch {
      elements.selectionStatus.textContent = "Copy was unavailable; select the command text manually.";
    }
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !elements.detailPanel.hidden) {
      elements.closeDetail.click();
      return;
    }
    const buttons = [...document.querySelectorAll("tbody .record")];
    const index = buttons.indexOf(document.activeElement);
    if (index < 0 || !["ArrowDown", "ArrowUp"].includes(event.key)) return;
    event.preventDefault();
    const offset = event.key === "ArrowDown" ? 1 : -1;
    buttons[Math.max(0, Math.min(buttons.length - 1, index + offset))]?.focus();
  });

  renderOverview();
  render();
})();
