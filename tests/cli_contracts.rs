use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures/reports").join(name)
}

fn git(repository: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read JSON file")).expect("parse JSON file")
}

fn assert_prompt_pack_safety(pack: &Path) -> Value {
    assert!(pack.join("prompt.md").is_file(), "missing prompt.md");
    assert!(pack.join("README.md").is_file(), "missing README.md");
    let manifest = read_json(&pack.join("manifest.json"));
    assert_eq!(manifest["schema_version"], 1);
    for name in ["context.json", "prompt.md", "README.md"] {
        assert_eq!(manifest["files"][name].as_str().map(str::len), Some(64));
    }

    let context = read_json(&pack.join("context.json"));
    assert_eq!(context["prompt_pack_version"], 1);
    assert_eq!(context["report_excerpt"]["schema_version"], 5);
    assert_eq!(
        context["provenance"]["report_sha256"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    assert_eq!(context["repository_context"]["included"], false);
    assert_eq!(context["repository_context"]["planning_usable"], false);
    assert_eq!(context["repository_context"]["execution_ready"], false);

    let boundary = context["boundary"].as_str().expect("prompt-pack boundary");
    assert!(boundary.contains("advisory only"));
    assert!(boundary.contains("must not rescore detector truth"));
    assert!(boundary.contains("mutate code, GitHub, or report data"));

    let prompt = fs::read_to_string(pack.join("prompt.md")).expect("read prompt.md");
    assert!(prompt.contains("Use only the facts in context.json"));
    assert!(prompt.contains("do not rescore detector truth"));
    assert!(prompt.contains("instead of inventing context"));

    let readme = fs::read_to_string(pack.join("README.md")).expect("read README.md");
    assert!(readme.contains("must not mutate code, GitHub, or detector truth"));
    assert!(readme.contains("must not rescore detector truth"));

    context
}

include!("cli_contracts/group_1.rs");
include!("cli_contracts/group_2.rs");
include!("cli_contracts/group_3.rs");
