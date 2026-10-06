#[test]
fn compare_text_ranks_score_movements_before_bounding_without_changing_machine_pages() {
    let directory = TempDir::new().expect("temporary report directory");
    let mut base = load_fixture("compare_base_report.json");
    let mut head = base.clone();
    head["repo"]["head_sha"] = json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let template = base["files"][0].clone();
    let mut base_files = Vec::new();
    let mut head_files = Vec::new();
    for index in 0..22 {
        let mut before = template.clone();
        before["path"] = json!(match index {
            20 => "z-improved.rs".to_string(),
            21 => "z-worsened\n::warning::.rs".to_string(),
            _ => format!("evidence-{index:02}.rs"),
        });
        before["slop_score"] = json!(20.0);
        let mut after = before.clone();
        if index < 20 {
            after["tokens"] = json!(before["tokens"].as_u64().expect("tokens") + 1);
        } else {
            after["slop_score"] = json!(if index == 20 { 10.0 } else { 30.0 });
            after["content_sha256"] =
                json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
            after["content_fingerprint"] = after["content_sha256"].clone();
        }
        base_files.push(before);
        head_files.push(after);
    }
    for (report, files) in [(&mut base, base_files), (&mut head, head_files)] {
        report["files"] = json!(files);
        report["folders"] = json!([]);
        report["action_queue"] = json!([
            {"path": "evidence-00.rs"},
            {"path": "z-improved.rs"}
        ]);
        report["collection_metadata"]["files"] =
            json!({"total": 22, "returned": 22, "limit": null, "truncated": false});
        report["collection_metadata"]["folders"] =
            json!({"total": 0, "returned": 0, "limit": null, "truncated": false});
    }
    let base = write_report(&directory, "base.json", &base);
    let head = write_report(&directory, "head.json", &head);
    for detail in [
        vec![],
        vec!["--detail", "summary"],
        vec!["--detail", "full", "--offset", "20", "--limit", "1"],
    ] {
        let output = command()
            .args(["compare", "--base"])
            .arg(&base)
            .arg("--head")
            .arg(&head)
            .args(["--top", "1"])
            .args(detail)
            .output()
            .expect("render score movement");
        assert_success(&output);
        let text = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
        assert!(
            text.contains("worsened_files=1, improved_files=1"),
            "{text}"
        );
        assert!(
            text.contains(
                "Top Worsened Files\n- z-worsened\\n::warning::.rs: 20.0 -> 30.0 (delta=10.0)"
            ),
            "{text}"
        );
        assert!(
            text.contains("Top Improved Files\n- z-improved.rs: 20.0 -> 10.0 (delta=-10.0)"),
            "{text}"
        );
        assert!(text.contains("Queue Movement\n- none"), "{text}");
        assert!(!text.contains("unchanged_position"), "{text}");
        assert!(
            !text.contains("\n::warning::"),
            "untrusted path created a physical line"
        );
    }

    let output = command()
        .args(["compare", "--base"])
        .arg(&base)
        .arg("--head")
        .arg(&head)
        .args(["--format", "json"])
        .output()
        .expect("bounded machine output");
    let payload = stdout_json(&output);
    assert_eq!(payload["summary"]["regression_count"], 1);
    assert_eq!(payload["pagination"]["file_deltas"]["total"], 22);
    assert_eq!(payload["pagination"]["file_deltas"]["returned"], 10);
    assert_eq!(payload["pagination"]["file_deltas"]["has_more"], true);
    assert!(
        payload["file_deltas"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["status"] == "evidence_drift")
    );
    for (offset, path, has_more) in [
        (20, "z-improved.rs", true),
        (21, "z-worsened\n::warning::.rs", false),
    ] {
        let output = command()
            .args(["compare", "--base"])
            .arg(&base)
            .arg("--head")
            .arg(&head)
            .args(["--format", "json", "--detail", "full", "--offset"])
            .arg(offset.to_string())
            .args(["--limit", "1"])
            .output()
            .expect("machine page");
        let page = stdout_json(&output);
        assert_eq!(page["file_deltas"].as_array().map(Vec::len), Some(1));
        assert_eq!(page["file_deltas"][0]["path"], path);
        assert_eq!(page["pagination"]["file_deltas"]["offset"], offset);
        assert_eq!(page["pagination"]["file_deltas"]["has_more"], has_more);
    }
}

#[test]
fn compare_text_queue_movement_omits_unchanged_positions_with_machine_detail_flags() {
    let directory = TempDir::new().expect("temporary report directory");
    let mut base = load_fixture("compare_base_report.json");
    let mut head = base.clone();
    head["repo"]["head_sha"] = json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    base["action_queue"] = json!([{"path": "src/a.py"}, {"path": "src/b.py"}]);
    head["action_queue"] = json!([{"path": "src/a.py"}, {"path": "src/gone.py"}]);
    let base = write_report(&directory, "base-queue.json", &base);
    let head = write_report(&directory, "head-queue.json", &head);
    let output = command()
        .args(["compare", "--base"])
        .arg(&base)
        .arg("--head")
        .arg(&head)
        .args([
            "--top", "1", "--detail", "full", "--offset", "1", "--limit", "1",
        ])
        .output()
        .expect("changed-only queue");
    assert_success(&output);
    let text = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let queue = text
        .split("Queue Movement\n")
        .nth(1)
        .expect("queue section");
    assert!(queue.contains("src/gone.py: newly_queued"), "{text}");
    assert!(
        !queue.contains("src/b.py") && !queue.contains("src/a.py"),
        "{text}"
    );
    assert!(!queue.contains("unchanged_position"), "{text}");
}
