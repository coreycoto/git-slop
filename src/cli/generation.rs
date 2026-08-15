fn run_html(repo_root: &Path, args: HtmlArgs) -> Result<i32> {
    let explicit_report = args.report.clone();
    let (loaded, report_path) = report_or_missing_with_currentness(
        repo_root,
        args.report.as_deref(),
        args.require_current,
    )?;
    let output = args.output.unwrap_or_else(|| {
        explicit_report
            .as_ref()
            .and_then(|path| path.parent())
            .map_or_else(|| config::latest_dir(repo_root), Path::to_path_buf)
            .join("report.html")
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    let array_records = |pointer: &str| {
        loaded
            .pointer(pointer)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let section_records = |pointer: &str| {
        loaded
            .pointer(pointer)
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|sections| sections.values())
            .filter_map(Value::as_array)
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
    };
    let embedded_limit = 5_000usize;
    let bounded = |records: &[Value]| records.iter().take(embedded_limit).cloned().collect::<Vec<_>>();
    let files = array_records("/files");
    let folders = array_records("/folders");
    let action_queue = array_records("/action_queue");
    let health = array_records("/health/findings");
    let relationships = section_records("/overlays/organization_health/relationships");
    let clusters = deduplicate_clusters(section_records(
        "/overlays/organization_health/clusters",
    ));
    let view_metadata = |records: &[Value]| {
        json!({
            "total": records.len(),
            "embedded": records.len().min(embedded_limit),
            "truncated": records.len() > embedded_limit
        })
    };
    let source_report = args
        .include_local_paths
        .then(|| relative_display(&report_path, repo_root));
    let payload = serde_json::to_string(&json!({
        "schema_version": loaded.get("schema_version"),
        "generated_at": loaded.get("generated_at"),
        "analyzed_revision_at": loaded.get("analyzed_revision_at"),
        "analyzer": loaded.get("analyzer"),
        "repo": loaded.get("repo"),
        "scope": loaded.get("scope"),
        "config_digests": {
            "config": loaded.pointer("/analyzer/config_digest"),
            "analysis": loaded.pointer("/analyzer/analysis_config_digest"),
            "evidence": loaded.pointer("/analyzer/evidence_config_digest"),
            "policy": loaded.pointer("/analyzer/policy_config_digest"),
            "presentation": loaded.pointer("/analyzer/presentation_config_digest")
        },
        "collection_metadata": loaded.get("collection_metadata"),
        "evidence_completeness": loaded.get("evidence_completeness"),
        "files": bounded(&files),
        "folders": bounded(&folders),
        "action_queue": bounded(&action_queue),
        "health": {
            "summary": loaded.pointer("/health/summary"),
            "findings": bounded(&health)
        },
        "organization": {
            "relationships": bounded(&relationships),
            "clusters": bounded(&clusters)
        },
        "embedded_evidence": {
            "record_limit_per_view": embedded_limit,
            "view_metadata": {
                "files": view_metadata(&files),
                "folders": view_metadata(&folders),
                "action_queue": view_metadata(&action_queue),
                "health": view_metadata(&health),
                "relationships": view_metadata(&relationships),
                "clusters": view_metadata(&clusters)
            }
        },
        "source_report": source_report
    }))?
    .replace("</", "<\\/");
    let csp_nonce = &hex::encode(sha2::Sha256::digest(payload.as_bytes()))[..24];
    let html = format!(
        r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'nonce-{nonce}'; script-src 'nonce-{nonce}'; base-uri 'none'; form-action 'none'">
  <title>Git Slop local report</title>
  <style nonce="{nonce}">{style}</style>
</head>
<body>
  <a class="skip-link" href="#report-main">Skip to report</a>
  <header class="hero shell">
    <div>
      <p class="eyebrow">Local repository evidence</p>
      <h1>Git Slop report</h1>
      <p id="descriptor" class="descriptor"></p>
    </div>
    <span id="schema-badge" class="hero-badge"></span>
  </header>
  <main id="report-main" class="shell">
    <section class="report-card" aria-labelledby="report-heading">
      <h2 id="report-heading" class="sr">Report explorer</h2>
      <p id="truncation" class="notice" role="status"></p>
      <p id="selection-status" class="sr" role="status" aria-live="polite"></p>
      <nav class="views" aria-label="Report view">
        <button type="button" data-view="files" aria-pressed="true">Files</button>
        <button type="button" data-view="folders" aria-pressed="false">Folders</button>
        <button type="button" data-view="queue" aria-pressed="false">Action queue</button>
        <button type="button" data-view="health" aria-pressed="false">Health findings</button>
        <button type="button" data-view="relationships" aria-pressed="false">Relationships</button>
        <button type="button" data-view="clusters" aria-pressed="false">Clusters</button>
      </nav>
      <div class="controls">
        <div class="control">
          <label for="query">Search records</label>
          <input id="query" type="search" placeholder="Search paths" autocomplete="off">
        </div>
        <div class="control">
          <label for="profile">Profile</label>
          <select id="profile"><option value="">All profiles</option></select>
        </div>
        <div class="control">
          <label for="classification">Classification</label>
          <select id="classification"><option value="">All classifications</option></select>
        </div>
        <div class="control">
          <label id="severity-label" for="severity">Maintenance band</label>
          <select id="severity"><option value="">All bands</option></select>
        </div>
      </div>
      <div class="results-toolbar">
        <div>
          <p id="count" aria-live="polite"></p>
          <p id="sort-state" class="muted" aria-live="polite"></p>
        </div>
        <div class="pagination" aria-label="Result pages">
          <button id="previous" type="button">Previous</button>
          <button id="next" type="button">Next</button>
        </div>
      </div>
      <div class="table-shell" tabindex="0" role="region" aria-label="Scrollable report table">
        <table>
          <caption class="sr">Git Slop report records</caption>
          <thead><tr id="headers"></tr></thead>
          <tbody id="rows"></tbody>
        </table>
      </div>
      <section id="detail-panel" class="detail-panel" aria-labelledby="detail-title" hidden>
        <div class="detail-heading">
          <div>
            <p class="eyebrow">Selected evidence</p>
            <h2 id="detail-title" tabindex="-1"></h2>
          </div>
          <button id="close-detail" type="button" aria-label="Close selected record">Close</button>
        </div>
        <p id="detail-summary" class="detail-summary"></p>
        <dl id="detail-metrics" class="metric-grid"></dl>
        <div class="detail-section">
          <h3>Supporting reasons</h3>
          <ul id="detail-reasons"></ul>
        </div>
        <div class="detail-section">
          <h3>Next commands</h3>
          <div id="detail-commands" class="command-list"></div>
        </div>
        <details>
          <summary>Raw record JSON</summary>
          <pre id="detail-raw"></pre>
        </details>
      </section>
      <details>
        <summary>Evidence and portability summary</summary>
        <pre id="evidence-summary"></pre>
      </details>
    </section>
  </main>
  <script id="report" type="application/json">{payload}</script>
  <script nonce="{nonce}">{script}</script>
</body>
</html>"##,
        nonce = csp_nonce,
        style = include_str!("html/report.css"),
        payload = payload,
        script = include_str!("html/report.js")
    );
    config::write_text_atomically(&output, html, false)?;
    println!("Wrote local HTML report to {}.", output.display());
    Ok(0)
}
