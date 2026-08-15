(() => {
  "use strict";

  const report = JSON.parse(document.getElementById("report").textContent);
  const params = new URLSearchParams(window.location.search);
  const pageSize = 100;
  const viewNames = {
    files: "Files",
    folders: "Folders",
    queue: "Action queue",
    health: "Health findings",
    relationships: "Relationships",
    clusters: "Clusters",
  };
  const recordsByView = {
    files: report.files ?? [],
    folders: report.folders ?? [],
    queue: report.action_queue ?? [],
    health: report.health?.findings ?? [],
    relationships: report.organization?.relationships ?? [],
    clusters: report.organization?.clusters ?? [],
  };
  const columns = {
    files: [
      ["path", "Path"],
      ["profile", "Profile"],
      ["classification", "Classification"],
      ["language", "Language"],
      ["slop_band", "Maintenance"],
      ["context_band", "Context"],
      ["slop_score", "Score"],
      ["tokens", "Tokens"],
    ],
    folders: [
      ["path", "Folder"],
      ["classification", "Classification"],
      ["slop_band", "Maintenance"],
      ["context_band", "Context"],
      ["slop_score", "Score"],
      ["tokens", "Tokens"],
    ],
    queue: [
      ["__rank", "Rank"],
      ["path", "Path"],
      ["profile", "Profile"],
      ["classification", "Classification"],
      ["severity", "Severity"],
      ["reason_codes", "Reasons"],
      ["evidence_status", "Evidence"],
      ["next_action", "Next action"],
    ],
    health: [
      ["severity", "Severity"],
      ["path", "Path"],
      ["title", "Finding"],
      ["message", "Message"],
    ],
    relationships: [
      ["id", "Relationship"],
      ["kind", "Kind"],
      ["source_path", "Source"],
      ["target_path", "Target"],
      ["confidence", "Confidence"],
      ["support_count", "Support"],
      ["evidence_lower_bound", "Lower bound"],
      ["evidence_score", "Evidence"],
    ],
    clusters: [
      ["id", "Cluster"],
      ["kind", "Kind"],
      ["member_count", "Count"],
      ["member_paths", "Members"],
      ["evidence_score", "Evidence"],
    ],
  };
  const sortDefaults = {
    files: { key: "slop_score", ascending: false },
    folders: { key: "slop_score", ascending: false },
    queue: { key: "__rank", ascending: true },
    health: { key: "severity", ascending: false },
    relationships: { key: "evidence_score", ascending: false },
    clusters: { key: "evidence_score", ascending: false },
  };
  const sortState = Object.fromEntries(
    Object.entries(sortDefaults).map(([key, value]) => [key, { ...value }]),
  );
  const severityOrder = {
    unknown: 0,
    notice: 1,
    low: 1,
    warning: 2,
    moderate: 2,
    high: 3,
    error: 4,
    critical: 5,
  };

  Object.values(recordsByView).forEach((records) => {
    records.forEach((record, index) => {
      Object.defineProperty(record, "__rank", {
        configurable: false,
        enumerable: false,
        value: index + 1,
      });
    });
  });

  const elements = {
    classification: document.getElementById("classification"),
    closeDetail: document.getElementById("close-detail"),
    count: document.getElementById("count"),
    detailCommands: document.getElementById("detail-commands"),
    detailMetrics: document.getElementById("detail-metrics"),
    detailPanel: document.getElementById("detail-panel"),
    detailRaw: document.getElementById("detail-raw"),
    detailReasons: document.getElementById("detail-reasons"),
    detailSummary: document.getElementById("detail-summary"),
    detailTitle: document.getElementById("detail-title"),
    headers: document.getElementById("headers"),
    next: document.getElementById("next"),
    previous: document.getElementById("previous"),
    profile: document.getElementById("profile"),
    query: document.getElementById("query"),
    rows: document.getElementById("rows"),
    selectionStatus: document.getElementById("selection-status"),
    severity: document.getElementById("severity"),
    severityLabel: document.getElementById("severity-label"),
    sortState: document.getElementById("sort-state"),
    truncation: document.getElementById("truncation"),
  };

  let view = Object.hasOwn(viewNames, params.get("view"))
    ? params.get("view")
    : "files";
  let page = Math.max(0, Number.parseInt(params.get("page") ?? "0", 10) || 0);
  let selectedIdentity = "";
  let selectedButton = null;
  let pendingRecord = params.get("record") ?? "";
  let restoreInitialFilters = true;

  if (columns[view].some(([key]) => key === params.get("sort"))) {
    sortState[view] = {
      key: params.get("sort"),
      ascending: params.get("dir") === "asc",
    };
  }

  elements.query.value = params.get("q") ?? "";
  document.getElementById("descriptor").textContent = `${report.repo?.repo_name ?? "repository"} · ${report.generated_at ?? "unknown time"}`;
  document.getElementById("schema-badge").textContent = `Schema ${report.schema_version ?? "unknown"}`;
  document.getElementById("evidence-summary").textContent = JSON.stringify(
    {
      config_digests: report.config_digests,
      completeness: report.evidence_completeness,
      collections: report.collection_metadata,
      embedded: report.embedded_evidence,
      source_report: report.source_report,
    },
    null,
    2,
  );

  function escapeHtml(value) {
    return String(value ?? "").replace(
      /[&<>"']/g,
      (character) =>
        ({
          "&": "&amp;",
          "<": "&lt;",
          ">": "&gt;",
          '"': "&quot;",
          "'": "&#39;",
        })[character],
    );
  }

  function displayValue(value) {
    if (Array.isArray(value)) return value.join(", ");
    if (value === true) return "Yes";
    if (value === false) return "No";
    if (value === null || value === undefined || value === "") return "—";
    if (typeof value === "number" && !Number.isInteger(value)) {
      return value.toLocaleString(undefined, { maximumFractionDigits: 3 });
    }
    if (typeof value === "object") return JSON.stringify(value);
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
    const key = view === "queue" ? "action_queue" : view === "health" ? "health" : view;
    const fallback = {
      embedded: recordsByView[view].length,
      total: recordsByView[view].length,
      truncated: false,
    };
    return report.embedded_evidence?.view_metadata?.[key] ?? fallback;
  }

  function syncUrl() {
    const url = new URL(window.location.href);
    url.search = "";
    const values = {
      view,
      q: elements.query.value,
      profile: elements.profile.value,
      classification: elements.classification.value,
      band: elements.severity.value,
      sort: sortState[view].key,
      dir: sortState[view].ascending ? "asc" : "desc",
      page: page === 0 ? "" : String(page),
      record: selectedIdentity,
    };
    Object.entries(values).forEach(([key, value]) => {
      if (value !== "") url.searchParams.set(key, value);
    });
    url.hash = selectedIdentity ? stableDomId(selectedIdentity) : "";
    window.history.replaceState(null, "", url);
  }

  function clearSelection({ updateUrl = false, announce = false } = {}) {
    if (selectedIdentity) {
      document
        .querySelector(`[data-identity="${CSS.escape(selectedIdentity)}"]`)
        ?.closest("tr")
        ?.removeAttribute("data-selected");
    }
    selectedIdentity = "";
    selectedButton = null;
    elements.detailPanel.hidden = true;
    if (announce) elements.selectionStatus.textContent = "Selection cleared.";
    else elements.selectionStatus.textContent = "";
    if (updateUrl) syncUrl();
  }

  function rebuildFilter(select, values, label) {
    const previous = select.value;
    const options = [...new Set(values.filter(Boolean).map(String))].sort((left, right) =>
      left.localeCompare(right),
    );
    select.replaceChildren(
      new Option(label, ""),
      ...options.map((value) => new Option(value, value)),
    );
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
      return `<button type="button" class="record" data-identity="${escapeHtml(identity)}" aria-controls="detail-panel"><code>${escapeHtml(displayValue(label))}</code></button>`;
    }
    return escapeHtml(displayValue(value));
  }

  function filterHaystack(record) {
    return [
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
      files: "File-level maintenance pressure and supporting signals from the selected report.",
      folders: "Folder-level maintenance pressure aggregated from the selected report.",
      queue: record.next_action ?? "Prioritized maintenance work from the report action queue.",
      health: record.message ?? "Repository health finding and its reported severity.",
      relationships: `${record.kind ?? "Relationship"} evidence connecting ${record.source_path ?? "the source"} and ${record.target_path ?? "the target"}.`,
      clusters: `${record.kind ?? "Cluster"} containing ${record.member_count ?? record.member_paths?.length ?? 0} reported members.`,
    };
    const metricKeys = {
      files: ["slop_score", "slop_band", "context_band", "profile", "classification", "language", "tokens"],
      folders: ["slop_score", "slop_band", "context_band", "classification", "tokens"],
      queue: ["__rank", "severity", "slop_score", "slop_band", "context_band", "evidence_status"],
      health: ["severity", "path", "title"],
      relationships: ["kind", "evidence_score", "evidence_lower_bound", "confidence", "support_count"],
      clusters: ["kind", "member_count", "evidence_score", "candidate_type"],
    }[view];
    const reasons = [
      ...(record.reason_codes ?? []),
      ...(record.member_paths ?? []),
      ...(record.source_relationship_ids ?? []),
    ];
    if (view === "relationships") {
      reasons.push(record.source_path, record.target_path);
    }
    const commands = [];
    if (path) {
      commands.push(`git slop explain --path ${shellQuote(path)}`);
      commands.push(`git slop plan --path ${shellQuote(path)}`);
    }
    if (view === "relationships" && record.id) {
      commands.unshift(`git slop explain --relationship ${shellQuote(record.id)}`);
    }
    if (view === "clusters" && record.id) {
      commands.unshift(`git slop explain --cluster ${shellQuote(record.id)}`);
    }
    return { commands, metricKeys, reasons: reasons.filter(Boolean), summary: summaries[view], title };
  }

  function selectRecord(record, button, { focus = false, updateUrl = true } = {}) {
    clearSelection();
    const content = detailContent(record);
    selectedIdentity = recordIdentity(view, record);
    selectedButton = button ?? null;
    button?.closest("tr")?.setAttribute("data-selected", "true");
    elements.detailTitle.textContent = content.title;
    elements.detailSummary.textContent = content.summary;
    elements.detailMetrics.innerHTML = content.metricKeys
      .map(
        (key) =>
          `<div class="metric"><dt>${escapeHtml(key.replaceAll("_", " "))}</dt><dd>${escapeHtml(displayValue(valueFor(record, key)))}</dd></div>`,
      )
      .join("");
    elements.detailReasons.innerHTML = content.reasons.length
      ? content.reasons.map((reason) => `<li>${escapeHtml(reason)}</li>`).join("")
      : "<li>No additional reasons were reported.</li>";
    elements.detailCommands.innerHTML = content.commands.length
      ? content.commands.map((command) => `<code>${escapeHtml(command)}</code>`).join("")
      : "<span class=\"muted\">No path-specific command is available for this record.</span>";
    elements.detailRaw.textContent = JSON.stringify(record, null, 2);
    elements.detailPanel.hidden = false;
    elements.selectionStatus.textContent = `Selected ${content.title}.`;
    if (updateUrl) syncUrl();
    if (focus) elements.detailTitle.focus({ preventScroll: false });
  }

  function render() {
    const source = recordsByView[view];
    const activeColumns = columns[view];
    const query = elements.query.value.trim().toLocaleLowerCase();

    document.querySelectorAll("[data-view]").forEach((button) => {
      button.setAttribute("aria-pressed", String(button.dataset.view === view));
    });
    rebuildFilter(elements.profile, source.map((record) => record.profile), "All profiles");
    rebuildFilter(
      elements.classification,
      source.map((record) => record.classification),
      "All classifications",
    );
    rebuildFilter(
      elements.severity,
      source.map((record) => record.slop_band ?? record.severity ?? record.health_band),
      "All bands",
    );
    if (restoreInitialFilters) {
      const requested = {
        profile: params.get("profile") ?? "",
        classification: params.get("classification") ?? "",
        severity: params.get("band") ?? "",
      };
      Object.entries(requested).forEach(([key, value]) => {
        const select = elements[key];
        if ([...select.options].some((option) => option.value === value)) select.value = value;
      });
      restoreInitialFilters = false;
    }

    const profileApplies = source.some((record) => record.profile);
    const classificationApplies = source.some((record) => record.classification);
    const bandApplies = source.some(
      (record) => record.slop_band ?? record.severity ?? record.health_band,
    );
    elements.profile.disabled = !profileApplies;
    elements.classification.disabled = !classificationApplies;
    elements.severity.disabled = !bandApplies;
    elements.severityLabel.textContent =
      view === "health" || view === "queue" ? "Severity or band" : "Maintenance band";

    const selected = source
      .filter(
        (record) =>
          (!query || filterHaystack(record).includes(query)) &&
          (!profileApplies || !elements.profile.value || record.profile === elements.profile.value) &&
          (!classificationApplies ||
            !elements.classification.value ||
            record.classification === elements.classification.value) &&
          (!bandApplies ||
            !elements.severity.value ||
            (record.slop_band ?? record.severity ?? record.health_band) === elements.severity.value),
      )
      .sort(compareValues);

    let deepLinkRecord = null;
    if (pendingRecord) {
      const index = selected.findIndex(
        (record) =>
          recordIdentity(view, record) === pendingRecord ||
          record.path === pendingRecord ||
          record.id === pendingRecord,
      );
      if (index >= 0) {
        page = Math.floor(index / pageSize);
        deepLinkRecord = selected[index];
      }
    }
    const pageCount = Math.max(1, Math.ceil(selected.length / pageSize));
    page = Math.min(page, pageCount - 1);
    const visible = selected.slice(page * pageSize, (page + 1) * pageSize);

    elements.headers.innerHTML = activeColumns
      .map(([key, label]) => {
        const active = key === sortState[view].key;
        const direction = active ? (sortState[view].ascending ? "ascending" : "descending") : "none";
        return `<th scope="col" aria-sort="${direction}"><button type="button" data-key="${escapeHtml(key)}">${escapeHtml(label)}</button></th>`;
      })
      .join("");
    elements.rows.innerHTML = visible.length
      ? visible
          .map((record) => {
            const identity = recordIdentity(view, record);
            return `<tr id="${stableDomId(identity)}">${activeColumns
              .map((column, index) => `<td>${renderCell(record, column[0], index)}</td>`)
              .join("")}</tr>`;
          })
          .join("")
      : `<tr><td class="empty-row" colspan="${activeColumns.length}">No records match these filters.</td></tr>`;

    const metadata = viewMetadata();
    elements.count.textContent = `${selected.length.toLocaleString()} filtered · ${metadata.embedded.toLocaleString()} embedded of ${metadata.total.toLocaleString()} total · page ${page + 1} of ${pageCount}`;
    elements.previous.disabled = page === 0;
    elements.next.disabled = page + 1 >= pageCount;
    const sortLabel =
      activeColumns.find(([key]) => key === sortState[view].key)?.[1] ?? sortState[view].key;
    elements.sortState.textContent = `Sorted by ${sortLabel}, ${sortState[view].ascending ? "ascending" : "descending"}.`;
    elements.truncation.textContent = metadata.truncated
      ? `${viewNames[view]} are truncated in this portable report.${report.source_report ? ` Open ${report.source_report} for complete evidence.` : " Regenerate from the source JSON to inspect complete evidence."}`
      : "";

    if (pendingRecord) {
      if (deepLinkRecord) {
        const identity = recordIdentity(view, deepLinkRecord);
        const button = document.querySelector(
          `.record[data-identity="${CSS.escape(identity)}"]`,
        );
        selectRecord(deepLinkRecord, button, { updateUrl: false });
        button?.scrollIntoView({ block: "nearest" });
      } else {
        elements.selectionStatus.textContent = `The selected record was not found in the filtered, embedded ${viewNames[view].toLocaleLowerCase()} view.`;
      }
      pendingRecord = "";
    }
    syncUrl();
  }

  elements.rows.addEventListener("click", (event) => {
    const pathLink = event.target.closest(".file-link");
    if (pathLink) {
      clearSelection();
      view = "files";
      page = 0;
      elements.query.value = pathLink.dataset.path;
      pendingRecord = `files:${pathLink.dataset.path}`;
      render();
      return;
    }
    const button = event.target.closest(".record");
    if (!button) return;
    const record = recordsByView[view].find(
      (candidate) => recordIdentity(view, candidate) === button.dataset.identity,
    );
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

  [elements.query, elements.profile, elements.classification, elements.severity].forEach(
    (control) => {
      control.addEventListener("input", () => {
        clearSelection();
        page = 0;
        render();
      });
    },
  );

  document.querySelectorAll("[data-view]").forEach((button) => {
    button.addEventListener("click", () => {
      clearSelection();
      view = button.dataset.view;
      page = 0;
      elements.profile.value = "";
      elements.classification.value = "";
      elements.severity.value = "";
      render();
    });
  });

  elements.previous.addEventListener("click", () => {
    clearSelection();
    page = Math.max(0, page - 1);
    render();
    document.querySelector(".table-shell").focus();
  });
  elements.next.addEventListener("click", () => {
    clearSelection();
    page += 1;
    render();
    document.querySelector(".table-shell").focus();
  });
  elements.closeDetail.addEventListener("click", () => {
    const restoreFocus = selectedButton;
    clearSelection({ announce: true, updateUrl: true });
    restoreFocus?.focus();
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

  render();
})();
