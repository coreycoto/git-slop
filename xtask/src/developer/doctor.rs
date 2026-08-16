use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Result, bail};

use super::{PrerequisiteReceipt, node_is_supported, print_doctor, rust_is_supported};

struct ToolCheck<'a> {
    name: &'a str,
    program: &'a str,
    args: &'a [&'a str],
    install: &'a str,
    supported: Option<fn(&str) -> bool>,
}

fn tool_receipt(repo_root: &Path, check: &ToolCheck<'_>) -> PrerequisiteReceipt {
    match Command::new(check.program)
        .current_dir(repo_root)
        .args(check.args)
        .output()
    {
        Ok(result) if result.status.success() => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);
            let version = stdout
                .lines()
                .chain(stderr.lines())
                .find(|line| !line.trim().is_empty())
                .unwrap_or("available")
                .trim()
                .to_string();
            if check.supported.is_none_or(|supported| supported(&version)) {
                PrerequisiteReceipt {
                    name: check.name.to_string(),
                    status: "ready",
                    detail: version,
                    recovery: None,
                }
            } else {
                PrerequisiteReceipt {
                    name: check.name.to_string(),
                    status: "unsupported",
                    detail: version,
                    recovery: Some(check.install.to_string()),
                }
            }
        }
        _ => PrerequisiteReceipt {
            name: check.name.to_string(),
            status: "missing",
            detail: "not available on PATH".to_string(),
            recovery: Some(check.install.to_string()),
        },
    }
}

fn nearest_existing_directory(path: &Path) -> Option<&Path> {
    let mut candidate = path;
    loop {
        if candidate.is_dir() {
            return Some(candidate);
        }
        candidate = candidate.parent()?;
    }
}

fn writable_directory_receipt(name: &str, requested: &Path) -> PrerequisiteReceipt {
    let Some(probe_root) = nearest_existing_directory(requested) else {
        return PrerequisiteReceipt {
            name: name.to_string(),
            status: "blocked",
            detail: format!("no existing parent for {}", requested.display()),
            recovery: Some(format!(
                "create a user-writable cache directory for {}",
                requested.display()
            )),
        };
    };
    let probe = probe_root.join(format!(
        ".git-slop-doctor-write-probe-{}",
        std::process::id()
    ));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(mut file) => {
            let wrote = file.write_all(b"git-slop doctor\n").is_ok();
            drop(file);
            let removed = fs::remove_file(&probe).is_ok();
            if wrote && removed {
                PrerequisiteReceipt {
                    name: name.to_string(),
                    status: "ready",
                    detail: requested.display().to_string(),
                    recovery: None,
                }
            } else {
                PrerequisiteReceipt {
                    name: name.to_string(),
                    status: "blocked",
                    detail: format!("write probe cleanup failed in {}", probe_root.display()),
                    recovery: Some(format!("remove {} and retry", probe.display())),
                }
            }
        }
        Err(error) => PrerequisiteReceipt {
            name: name.to_string(),
            status: "blocked",
            detail: format!("{}: {error}", requested.display()),
            recovery: Some(format!(
                "grant the current user write access to {} (for example: chmod u+w '{}')",
                probe_root.display(),
                probe_root.display()
            )),
        },
    }
}

pub(super) fn collect_prerequisites(repo_root: &Path) -> Vec<PrerequisiteReceipt> {
    let checks = [
        ToolCheck {
            name: "Git",
            program: "git",
            args: &["--version"],
            install: "https://git-scm.com/downloads",
            supported: None,
        },
        ToolCheck {
            name: "Cargo",
            program: "cargo",
            args: &["--version"],
            install: "install Rust 1.85+ with rustup",
            supported: None,
        },
        ToolCheck {
            name: "Rust",
            program: "rustc",
            args: &["--version"],
            install: "rustup toolchain install 1.85",
            supported: Some(rust_is_supported),
        },
        ToolCheck {
            name: "Node.js",
            program: "node",
            args: &["--version"],
            install: "install Node.js 24 or newer",
            supported: Some(node_is_supported),
        },
        ToolCheck {
            name: "Bash",
            program: "bash",
            args: &["--version"],
            install: "install Bash for wrapper validation",
            supported: None,
        },
        ToolCheck {
            name: "actionlint",
            program: "actionlint",
            args: &["-version"],
            install: "brew install actionlint or use its published binary",
            supported: None,
        },
        ToolCheck {
            name: "cargo-deny",
            program: "cargo",
            args: &["deny", "--version"],
            install: "cargo install cargo-deny --locked",
            supported: None,
        },
    ];
    let mut receipts = checks
        .iter()
        .map(|check| tool_receipt(repo_root, check))
        .collect::<Vec<_>>();
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
    if let Some(cargo_home) = cargo_home {
        receipts.push(writable_directory_receipt("Cargo cache", &cargo_home));
        receipts.push(writable_directory_receipt(
            "advisory cache",
            &cargo_home.join("advisory-dbs"),
        ));
    } else {
        for name in ["Cargo cache", "advisory cache"] {
            receipts.push(PrerequisiteReceipt {
                name: name.to_string(),
                status: "blocked",
                detail: "CARGO_HOME and HOME are unavailable".to_string(),
                recovery: Some("set CARGO_HOME to a user-writable directory".to_string()),
            });
        }
    }
    receipts
}

pub fn doctor(repo_root: &Path, json_output: bool) -> Result<()> {
    let started = Instant::now();
    let receipts = collect_prerequisites(repo_root);
    let ready = receipts.iter().all(|receipt| receipt.status == "ready");
    if json_output {
        print_doctor(
            if ready { "ready" } else { "blocked" },
            &receipts,
            started.elapsed().as_millis(),
        )?;
    } else {
        println!("Developer environment:");
        for receipt in &receipts {
            println!(
                "- {:<16} {:<11} {}",
                receipt.name, receipt.status, receipt.detail
            );
            if let Some(recovery) = &receipt.recovery {
                println!("  recovery: {recovery}");
            }
        }
    }
    if ready {
        if !json_output {
            println!("Developer environment is ready for `cargo xtask verify-changed`.");
        }
        Ok(())
    } else {
        let blocked = receipts
            .iter()
            .filter(|receipt| receipt.status != "ready")
            .map(|receipt| receipt.name.as_str())
            .collect::<Vec<_>>();
        bail!(
            "developer prerequisite(s) not ready: {}",
            blocked.join(", ")
        )
    }
}
