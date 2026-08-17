use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result, bail};

mod doctor;
mod receipt;

use doctor::collect_prerequisites;
pub use doctor::doctor;

use receipt::{
    GateReceipt, PrerequisiteReceipt, bounded_output, print_ci, print_doctor, print_verify_changed,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gate {
    PublicRust,
    MaintainerContracts,
    Action,
    Wrapper,
    WorkflowLint,
    SupplyChain,
}

const ALL_GATES: [Gate; 6] = [
    Gate::PublicRust,
    Gate::MaintainerContracts,
    Gate::Action,
    Gate::Wrapper,
    Gate::WorkflowLint,
    Gate::SupplyChain,
];

impl Gate {
    fn label(self) -> &'static str {
        match self {
            Self::PublicRust => "public-rust",
            Self::MaintainerContracts => "maintainer-contracts",
            Self::Action => "action",
            Self::Wrapper => "agent-plugins-wrapper",
            Self::WorkflowLint => "workflow-lint",
            Self::SupplyChain => "supply-chain",
        }
    }
}

fn is_root_contract(path: &str) -> bool {
    matches!(
        path,
        "AGENTS.md"
            | "Brewfile"
            | "CONTRIBUTING.md"
            | "Cargo.toml"
            | "Cargo.lock"
            | "README.md"
            | "build.rs"
            | "deny.toml"
    )
}

fn is_root_documentation(path: &str) -> bool {
    !path.contains('/') && path.ends_with(".md")
}

pub fn classify_paths(paths: &[String]) -> BTreeSet<Gate> {
    let mut gates = BTreeSet::new();
    for path in paths {
        let mut matched = false;
        if is_root_contract(path)
            || is_root_documentation(path)
            || path.starts_with("src/")
            || path.starts_with("tests/")
            || path.starts_with("docs/")
            || path.starts_with("schemas/")
            || path.starts_with("man/")
            || path.starts_with(".slop/")
        {
            gates.insert(Gate::PublicRust);
            matched = true;
        }
        if is_root_contract(path)
            || is_root_documentation(path)
            || path == "action.yml"
            || [
                "xtask/",
                ".cargo/",
                ".codex/",
                ".agents/",
                "plugins/",
                "config/",
                ".github/",
                "docs/",
                "schemas/",
                "man/",
                "scripts/",
                ".slop/",
                "action/",
                "assets/",
                "completions/",
            ]
            .iter()
            .any(|prefix| path.starts_with(prefix))
        {
            gates.insert(Gate::MaintainerContracts);
            matched = true;
        }
        if path == "action.yml" || path.starts_with("action/") {
            gates.insert(Gate::Action);
            matched = true;
        }
        if path.starts_with("scripts/with-agent-plugins") {
            gates.insert(Gate::Wrapper);
            matched = true;
        }
        if path == "action.yml"
            || path.starts_with(".github/workflows/")
            || path.starts_with(".github/workflow-sources/")
        {
            gates.insert(Gate::WorkflowLint);
            matched = true;
        }
        if matches!(
            path.as_str(),
            "Cargo.toml" | "Cargo.lock" | "deny.toml" | "Brewfile"
        ) || path == "xtask/Cargo.toml"
            || path == "xtask/Cargo.lock"
        {
            gates.insert(Gate::SupplyChain);
            matched = true;
        }
        if !matched {
            gates.insert(Gate::MaintainerContracts);
        }
    }
    gates
}

fn output(repo_root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "{program} {} exited with {}{}",
            args.join(" "),
            output.status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn changed_paths(repo_root: &Path, explicit_base: Option<&str>) -> Result<Vec<String>> {
    let base = explicit_base
        .map(ToOwned::to_owned)
        .or_else(|| {
            output(
                repo_root,
                "git",
                &[
                    "rev-parse",
                    "--abbrev-ref",
                    "--symbolic-full-name",
                    "@{upstream}",
                ],
            )
            .ok()
            .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            output(
                repo_root,
                "git",
                &["rev-parse", "--verify", "--quiet", "origin/main"],
            )
            .ok()
            .filter(|value| !value.is_empty())
            .map(|_| "origin/main".to_string())
        });
    let diff_base = if let Some(base) = base {
        let head_tree = output(repo_root, "git", &["rev-parse", "HEAD^{tree}"])?;
        let base_tree_revision = format!("{base}^{{tree}}");
        let base_tree = output(
            repo_root,
            "git",
            &["rev-parse", "--verify", &base_tree_revision],
        )?;
        if head_tree == base_tree {
            // A branch that has returned to the base tree has no committed
            // content delta. Diffing HEAD still retains staged and unstaged
            // work without walking a potentially deep merge-base history.
            "HEAD".to_string()
        } else {
            output(repo_root, "git", &["merge-base", "HEAD", &base])?
        }
    } else if output(
        repo_root,
        "git",
        &["rev-parse", "--verify", "--quiet", "HEAD^"],
    )
    .is_ok()
    {
        "HEAD^".to_string()
    } else {
        "HEAD".to_string()
    };
    let mut paths = output(repo_root, "git", &["diff", "--name-only", &diff_base])?
        .lines()
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    paths.extend(
        output(
            repo_root,
            "git",
            &["ls-files", "--others", "--exclude-standard"],
        )?
        .lines()
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned),
    );
    Ok(paths.into_iter().collect())
}

fn run(repo_root: &Path, program: &str, args: &[&str], quiet: bool) -> Result<()> {
    if !quiet {
        println!("$ {program} {}", args.join(" "));
        let status = Command::new(program)
            .current_dir(repo_root)
            .args(args)
            .status()
            .with_context(|| format!("failed to run {program}"))?;
        return if status.success() {
            Ok(())
        } else {
            bail!("{program} {} exited with {status}", args.join(" "))
        };
    }
    let output = Command::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let captured = [
            ("stdout", bounded_output(&output.stdout)),
            ("stderr", bounded_output(&output.stderr)),
        ]
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(label, value)| format!("{label}:\n{value}"))
        .collect::<Vec<_>>()
        .join("\n");
        bail!(
            "{program} {} exited with {}{}",
            args.join(" "),
            output.status,
            if captured.is_empty() {
                String::new()
            } else {
                format!(":\n{captured}")
            }
        )
    }
}

fn action_tests(repo_root: &Path) -> Result<Vec<String>> {
    let mut tests = fs::read_dir(repo_root.join("action"))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".test.mjs"))
        .map(|name| format!("action/{name}"))
        .collect::<Vec<_>>();
    tests.sort();
    Ok(tests)
}

fn run_gate(repo_root: &Path, gate: Gate, quiet: bool) -> Result<()> {
    if !quiet {
        println!("\n== {} ==", gate.label());
    }
    match gate {
        Gate::PublicRust => {
            run(
                repo_root,
                "cargo",
                &["fmt", "-p", "git-slop", "--", "--check"],
                quiet,
            )?;
            run(
                repo_root,
                "cargo",
                &[
                    "clippy",
                    "-p",
                    "git-slop",
                    "--all-targets",
                    "--all-features",
                    "--locked",
                    "--",
                    "-D",
                    "warnings",
                ],
                quiet,
            )?;
            run(
                repo_root,
                "cargo",
                &[
                    "test",
                    "-p",
                    "git-slop",
                    "--all-targets",
                    "--all-features",
                    "--locked",
                ],
                quiet,
            )
        }
        Gate::MaintainerContracts => {
            run(
                repo_root,
                "cargo",
                &[
                    "fmt",
                    "--manifest-path",
                    "xtask/Cargo.toml",
                    "--all",
                    "--",
                    "--check",
                ],
                quiet,
            )?;
            run(
                repo_root,
                "cargo",
                &[
                    "clippy",
                    "--manifest-path",
                    "xtask/Cargo.toml",
                    "--all-targets",
                    "--all-features",
                    "--locked",
                    "--",
                    "-D",
                    "warnings",
                ],
                quiet,
            )?;
            run(
                repo_root,
                "cargo",
                &[
                    "test",
                    "--manifest-path",
                    "xtask/Cargo.toml",
                    "--all-targets",
                    "--all-features",
                    "--locked",
                ],
                quiet,
            )?;
            let mut errors = crate::codex::validate(repo_root, false);
            errors.extend(crate::workflows::validate(repo_root));
            errors.extend(crate::issue_forms::validate(repo_root));
            errors.extend(crate::repository::validate(repo_root));
            errors.extend(crate::distribution::validate(repo_root));
            if quiet {
                if errors.is_empty() {
                    Ok(())
                } else {
                    bail!(
                        "repository contract validation failed:\n{}",
                        errors.join("\n")
                    )
                }
            } else {
                crate::finish_validation("Repository", errors)
            }
        }
        Gate::Action => {
            let tests = action_tests(repo_root)?;
            let mut args = vec!["--test".to_string()];
            args.extend(tests);
            run(
                repo_root,
                "node",
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
                quiet,
            )
        }
        Gate::Wrapper => run(
            repo_root,
            "bash",
            &["scripts/with-agent-plugins.test.sh"],
            quiet,
        ),
        Gate::WorkflowLint => run(repo_root, "actionlint", &[], quiet),
        Gate::SupplyChain => run(
            repo_root,
            "cargo",
            &["deny", "check", "advisories", "licenses", "sources"],
            quiet,
        ),
    }
}

pub fn verify_changed(
    repo_root: &Path,
    base: Option<&str>,
    dry_run: bool,
    json_output: bool,
) -> Result<()> {
    let started = Instant::now();
    let prerequisites = collect_prerequisites(repo_root);
    let paths = changed_paths(repo_root, base)?;
    if paths.is_empty() && !json_output {
        println!("No changed files detected.");
        return Ok(());
    }
    if !json_output {
        println!("Changed files ({}):", paths.len());
        for path in &paths {
            println!("- {path}");
        }
    }
    let selected = classify_paths(&paths);
    let selected_labels = ALL_GATES
        .iter()
        .copied()
        .filter(|gate| selected.contains(gate))
        .map(Gate::label)
        .collect::<Vec<_>>();
    let skipped_labels = ALL_GATES
        .iter()
        .copied()
        .filter(|gate| !selected.contains(gate))
        .map(Gate::label)
        .collect::<Vec<_>>();
    if !json_output {
        println!("Selected gates:");
        for gate in ALL_GATES {
            println!(
                "- {:<24} {}",
                gate.label(),
                if selected.contains(&gate) {
                    "run"
                } else {
                    "skip"
                }
            );
        }
    }
    if dry_run {
        if json_output {
            let gates = ALL_GATES
                .iter()
                .map(|gate| GateReceipt {
                    name: gate.label(),
                    status: if selected.contains(gate) {
                        "selected"
                    } else {
                        "skipped"
                    },
                    elapsed_ms: 0,
                })
                .collect::<Vec<_>>();
            print_verify_changed(
                "dry-run",
                &paths,
                &gates,
                &selected_labels,
                &skipped_labels,
                &prerequisites,
                true,
                started.elapsed().as_millis(),
                None,
                None,
            )?;
        }
        return Ok(());
    }
    let mut gates = Vec::new();
    for gate in ALL_GATES {
        if !selected.contains(&gate) {
            gates.push(GateReceipt {
                name: gate.label(),
                status: "skipped",
                elapsed_ms: 0,
            });
            continue;
        }
        let gate_started = Instant::now();
        if let Err(error) = run_gate(repo_root, gate, json_output) {
            gates.push(GateReceipt {
                name: gate.label(),
                status: "failed",
                elapsed_ms: gate_started.elapsed().as_millis(),
            });
            if json_output {
                print_verify_changed(
                    "failed",
                    &paths,
                    &gates,
                    &selected_labels,
                    &skipped_labels,
                    &prerequisites,
                    false,
                    started.elapsed().as_millis(),
                    Some(gate.label()),
                    Some(&format!("{error:#}")),
                )?;
            }
            return Err(error);
        }
        gates.push(GateReceipt {
            name: gate.label(),
            status: "passed",
            elapsed_ms: gate_started.elapsed().as_millis(),
        });
    }
    if json_output {
        print_verify_changed(
            "passed",
            &paths,
            &gates,
            &selected_labels,
            &skipped_labels,
            &prerequisites,
            false,
            started.elapsed().as_millis(),
            None,
            None,
        )?;
    }
    Ok(())
}

pub fn ci(repo_root: &Path, quiet: bool, json_output: bool) -> Result<()> {
    if !quiet {
        println!(
            "Running the complete local validation matrix ({} gates).",
            ALL_GATES.len()
        );
    }
    let started = Instant::now();
    let prerequisites = collect_prerequisites(repo_root);
    let mut gates = Vec::new();
    for gate in ALL_GATES {
        let gate_started = Instant::now();
        if let Err(error) = run_gate(repo_root, gate, quiet) {
            gates.push(GateReceipt {
                name: gate.label(),
                status: "failed",
                elapsed_ms: gate_started.elapsed().as_millis(),
            });
            if json_output {
                print_ci(
                    "failed",
                    &gates,
                    &prerequisites,
                    started.elapsed().as_millis(),
                    Some(gate.label()),
                    Some(&format!("{error:#}")),
                )?;
            }
            return Err(error);
        }
        gates.push(GateReceipt {
            name: gate.label(),
            status: "passed",
            elapsed_ms: gate_started.elapsed().as_millis(),
        });
    }
    if json_output {
        print_ci(
            "passed",
            &gates,
            &prerequisites,
            started.elapsed().as_millis(),
            None,
            None,
        )?;
    } else {
        println!("Complete local validation matrix passed.");
    }
    Ok(())
}

fn numeric_version(value: &str) -> Vec<u64> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .take(3)
        .collect()
}

fn rust_is_supported(value: &str) -> bool {
    numeric_version(value).as_slice() >= [1, 85].as_slice()
}

fn node_is_supported(value: &str) -> bool {
    numeric_version(value)
        .first()
        .is_some_and(|major| *major >= 24)
}

#[cfg(test)]
mod tests {
    use super::{
        Gate, bounded_output, changed_paths, classify_paths, node_is_supported, rust_is_supported,
    };
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

    fn gates(paths: &[&str]) -> BTreeSet<Gate> {
        classify_paths(
            &paths
                .iter()
                .map(|path| (*path).to_string())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn docs_schemas_man_and_state_cannot_bypass_public_or_contract_validation() {
        for path in [
            "docs/commands.md",
            "schemas/check-1.json",
            "man/git-slop.1",
            ".slop/.gitignore",
        ] {
            let selected = gates(&[path]);
            assert!(selected.contains(&Gate::PublicRust), "{path}");
            assert!(selected.contains(&Gate::MaintainerContracts), "{path}");
        }
    }

    #[test]
    fn repeated_public_paths_do_not_select_unrelated_gates() {
        assert_eq!(
            gates(&["src/lib.rs", "src/model.rs"]),
            BTreeSet::from([Gate::PublicRust])
        );
    }

    #[test]
    fn action_workflow_wrapper_and_supply_chain_select_exact_extra_gates() {
        let action = gates(&["action.yml"]);
        assert!(action.contains(&Gate::Action));
        assert!(action.contains(&Gate::MaintainerContracts));
        assert!(action.contains(&Gate::WorkflowLint));
        assert!(gates(&[".github/workflows/ci.yml"]).contains(&Gate::WorkflowLint));
        assert!(gates(&["scripts/with-agent-plugins.sh"]).contains(&Gate::Wrapper));
        for path in ["Cargo.lock", "deny.toml", "Brewfile"] {
            assert!(gates(&[path]).contains(&Gate::SupplyChain), "{path}");
        }
        assert!(gates(&["SECURITY.md"]).contains(&Gate::PublicRust));
        assert!(gates(&["new-owned-surface.txt"]).contains(&Gate::MaintainerContracts));
    }

    #[test]
    fn contributor_version_requirements_are_enforced() {
        assert!(rust_is_supported("rustc 1.85.0 (fixture)"));
        assert!(rust_is_supported("rustc 2.0.0 (fixture)"));
        assert!(!rust_is_supported("rustc 1.84.1 (fixture)"));
        assert!(node_is_supported("v24.1.0"));
        assert!(!node_is_supported("v22.18.0"));
    }

    #[test]
    fn quiet_failure_output_preserves_short_details_and_bounds_long_logs() {
        assert_eq!(bounded_output(b" assertion failed \n"), "assertion failed");
        let long = "a".repeat(12_001);
        let bounded = bounded_output(long.as_bytes());
        assert!(bounded.starts_with("[earlier output truncated]\n"));
        assert_eq!(bounded.lines().nth(1).unwrap().len(), 12_000);
    }

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn equal_tree_fast_path_keeps_uncommitted_and_untracked_changes() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        git(root, &["init", "--quiet"]);
        git(root, &["config", "user.name", "Git Slop Test"]);
        git(root, &["config", "user.email", "git-slop@example.invalid"]);
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        git(root, &["add", "tracked.txt"]);
        git(root, &["commit", "--quiet", "-m", "base"]);
        let base = git(root, &["rev-parse", "HEAD"]);

        fs::write(root.join("tracked.txt"), "temporary branch content\n").unwrap();
        git(root, &["commit", "--quiet", "-am", "change"]);
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        git(root, &["commit", "--quiet", "-am", "restore base tree"]);
        assert_eq!(
            git(root, &["rev-parse", "HEAD^{tree}"]),
            git(root, &["rev-parse", &format!("{base}^{{tree}}")])
        );

        fs::write(root.join("tracked.txt"), "uncommitted\n").unwrap();
        fs::write(root.join("untracked.txt"), "new\n").unwrap();
        assert_eq!(
            changed_paths(root, Some(&base)).unwrap(),
            ["tracked.txt", "untracked.txt"]
        );
    }
}
