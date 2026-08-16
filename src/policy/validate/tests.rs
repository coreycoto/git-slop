mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fixture_pack(root: &Path) {
        fs::create_dir_all(root.join("policies")).expect("policy directory");
        fs::write(
            root.join("git-slop-policy.yaml"),
            r#"schema_version: 1
id: com.example.test-policy
name: Test policy
description: Test-only policy pack.
version: 1.0.0
license: CC0-1.0
min_git_slop_version: 0.16.0
entrypoints: [policies/rule.md]
applicability: [advise]
rules:
  - id: com.example.test-policy.rule
    text: Cite the supplied evidence.
    applicability: [advise]
    severity: error
    consequence: revise
    required_evidence: [citations]
    insufficient_evidence: abstain
"#,
        )
        .expect("manifest");
        fs::write(root.join("policies/rule.md"), "# Rule\n").expect("entrypoint");
    }

    #[test]
    fn path_and_text_normalization_fail_closed() {
        assert!(safe_relative_path("policies/../README.md", "policies/", &[".md"]).is_err());
        assert!(safe_relative_path("/policies/rule.md", "policies/", &[".md"]).is_err());
        assert!(safe_relative_path("policies/rule.sh", "policies/", &[".md"]).is_err());
        assert!(normalized_text(vec![0xff], "fixture").is_err());
        assert!(normalized_text(b"unsafe\0text".to_vec(), "fixture").is_err());
        assert!(normalized_text("e\u{301}".as_bytes().to_vec(), "fixture").is_err());
        assert_eq!(
            normalized_text(b"one\r\ntwo\r".to_vec(), "fixture").expect("normalized text"),
            "one\ntwo\n"
        );
    }

    #[test]
    fn pack_loader_enforces_file_size_count_and_declared_paths() {
        let oversized = tempdir().expect("oversized pack");
        fixture_pack(oversized.path());
        fs::write(
            oversized.path().join("policies/rule.md"),
            vec![b'x'; MAX_FILE_BYTES + 1],
        )
        .expect("oversized entrypoint");
        assert!(
            load_and_validate_pack(oversized.path())
                .expect_err("oversized pack must fail")
                .to_string()
                .contains("exceeds")
        );

        let undeclared = tempdir().expect("undeclared pack");
        fixture_pack(undeclared.path());
        fs::write(undeclared.path().join("unexpected.txt"), "undeclared\n")
            .expect("undeclared file");
        assert!(
            load_and_validate_pack(undeclared.path())
                .expect_err("undeclared file must fail")
                .to_string()
                .contains("undeclared")
        );

        let excessive = tempdir().expect("excessive pack");
        fixture_pack(excessive.path());
        for index in 0..MAX_FILES {
            fs::write(excessive.path().join(format!("extra-{index}.txt")), "x")
                .expect("extra file");
        }
        assert!(
            load_and_validate_pack(excessive.path())
                .expect_err("excessive file count must fail")
                .to_string()
                .contains("file limit")
        );
    }

    #[test]
    fn duplicate_rules_and_non_normalized_optional_text_fail_closed() {
        let duplicate = tempdir().expect("duplicate pack");
        fixture_pack(duplicate.path());
        let manifest = fs::read_to_string(duplicate.path().join("git-slop-policy.yaml"))
            .expect("manifest");
        let duplicated_rule = manifest
            .split("  - id:")
            .nth(1)
            .expect("rule body");
        fs::write(
            duplicate.path().join("git-slop-policy.yaml"),
            format!("{manifest}  - id:{duplicated_rule}"),
        )
        .expect("duplicate manifest");
        assert!(
            load_and_validate_pack(duplicate.path())
                .expect_err("duplicate rule must fail")
                .to_string()
                .contains("duplicate policy rule id")
        );

        let unicode = tempdir().expect("Unicode pack");
        fixture_pack(unicode.path());
        fs::write(unicode.path().join("README.md"), "Cafe\u{301}\n")
            .expect("decomposed README");
        assert!(
            load_and_validate_pack(unicode.path())
                .expect_err("decomposed Unicode must fail")
                .to_string()
                .contains("NFC")
        );
    }
}
