use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use globset::Glob;
use serde_json::Value;

fn candidate_directories(candidates: &[String]) -> Vec<PathBuf> {
    let mut directories = BTreeSet::from([PathBuf::new()]);
    for candidate in candidates {
        let mut parent = Path::new(candidate).parent();
        while let Some(path) = parent {
            directories.insert(path.to_path_buf());
            parent = path.parent();
        }
    }
    directories.into_iter().collect()
}

fn commands_for_directory(
    directory: &Path,
    exists: impl Fn(&Path) -> bool,
    has_dotnet_project: impl Fn(&Path) -> bool,
) -> Vec<&'static str> {
    if exists(&directory.join("Cargo.toml")) {
        vec![
            "cargo fmt --all -- --check",
            "cargo clippy --all-targets --all-features -- -D warnings",
            "cargo test --all-targets",
        ]
    } else if exists(&directory.join("go.mod")) {
        vec!["go test ./..."]
    } else if exists(&directory.join("pyproject.toml")) {
        vec!["pytest"]
    } else if exists(&directory.join("package.json")) {
        if exists(&directory.join("bun.lock")) || exists(&directory.join("bun.lockb")) {
            vec!["bun test"]
        } else if exists(&directory.join("pnpm-lock.yaml")) {
            vec!["pnpm test"]
        } else if exists(&directory.join("yarn.lock")) {
            vec!["yarn test"]
        } else {
            vec!["npm test"]
        }
    } else if exists(&directory.join("pom.xml")) {
        vec!["mvn test"]
    } else if exists(&directory.join("gradlew")) {
        vec!["./gradlew test"]
    } else if has_dotnet_project(directory) {
        vec!["dotnet test"]
    } else if exists(&directory.join("Makefile")) {
        vec!["make test"]
    } else {
        Vec::new()
    }
}

fn command_family(command: &str) -> String {
    let command = command
        .rsplit_once("&&")
        .map_or(command, |(_, command)| command)
        .trim();
    let words = command.split_whitespace().collect::<Vec<_>>();
    match words.as_slice() {
        ["cargo", operation, ..] if matches!(*operation, "fmt" | "clippy" | "test") => {
            format!("cargo {operation}")
        }
        [runner, "test", ..] if matches!(*runner, "bun" | "npm" | "pnpm" | "yarn" | "go") => {
            format!("{runner} test")
        }
        ["pytest", ..] => "pytest".to_string(),
        ["mvn", "test", ..] => "mvn test".to_string(),
        ["dotnet", "test", ..] => "dotnet test".to_string(),
        _ => command.to_string(),
    }
}

fn discover(
    candidates: &[String],
    configured_commands: &[String],
    exists: impl Fn(&Path) -> bool + Copy,
    has_dotnet_project: impl Fn(&Path) -> bool + Copy,
) -> Vec<String> {
    let mut commands = configured_commands
        .iter()
        .filter(|command| !command.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    let configured_families = commands
        .iter()
        .map(|command| command_family(command))
        .collect::<BTreeSet<_>>();
    for directory in candidate_directories(candidates) {
        let prefix = if directory.as_os_str().is_empty() {
            String::new()
        } else {
            format!("cd {} && ", directory.to_string_lossy())
        };
        for command in commands_for_directory(&directory, exists, has_dotnet_project) {
            let command = format!("{prefix}{command}");
            if !configured_families.contains(&command_family(&command))
                && !commands.contains(&command)
            {
                commands.push(command);
            }
        }
    }
    commands
}

pub(super) fn from_worktree(
    root: &Path,
    candidates: &[String],
    configured_commands: &[String],
) -> Vec<String> {
    discover(
        candidates,
        configured_commands,
        |path| root.join(path).is_file(),
        |directory| {
            std::fs::read_dir(root.join(directory))
                .ok()
                .is_some_and(|entries| {
                    entries.filter_map(Result::ok).any(|entry| {
                        matches!(
                            entry.path().extension().and_then(|value| value.to_str()),
                            Some("sln" | "csproj")
                        )
                    })
                })
        },
    )
}

pub(super) fn from_report_paths(
    report: &Value,
    repository_paths: &BTreeSet<String>,
    candidates: &[String],
    configured_commands: &[String],
) -> Vec<String> {
    let mut commands = discover(
        candidates,
        configured_commands,
        |path| repository_paths.contains(&path.to_string_lossy().replace('\\', "/")),
        |directory| {
            let prefix = if directory.as_os_str().is_empty() {
                String::new()
            } else {
                format!("{}/", directory.to_string_lossy().replace('\\', "/"))
            };
            repository_paths.iter().any(|path| {
                path.strip_prefix(&prefix)
                    .filter(|relative| !relative.contains('/'))
                    .is_some_and(|relative| {
                        relative.ends_with(".sln") || relative.ends_with(".csproj")
                    })
            })
        },
    );
    let mut push = |command: String| {
        if !command.trim().is_empty() && !commands.contains(&command) {
            commands.push(command);
        }
    };
    for mapping in report
        .pointer("/config/verification/path_commands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(pattern) = mapping.get("path_glob").and_then(Value::as_str) else {
            continue;
        };
        let Some(command) = mapping.get("command").and_then(Value::as_str) else {
            continue;
        };
        if Glob::new(pattern).ok().is_some_and(|glob| {
            let matcher = glob.compile_matcher();
            candidates.iter().any(|path| matcher.is_match(path))
        }) {
            push(command.to_owned());
        }
    }
    for path in candidates {
        let Some(record) = report
            .get("files")
            .and_then(Value::as_array)
            .and_then(|files| files.iter().find(|record| record["path"] == *path))
        else {
            continue;
        };
        if let Some(command) = record
            .pointer("/generated_provenance/verification_command")
            .and_then(Value::as_str)
        {
            push(command.to_owned());
        }
        for test in record
            .pointer("/overlays/verification/nearby_test_paths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if let Some(name) = test
                .strip_prefix("tests/")
                .and_then(|path| path.strip_suffix(".rs"))
                .filter(|path| !path.contains('/'))
            {
                push(format!("cargo test --test {name}"));
            }
        }
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::from_report_paths;
    use std::collections::BTreeSet;

    #[test]
    fn discovers_nested_package_managers_and_build_systems() {
        let paths = [
            "Cargo.toml",
            "packages/web/package.json",
            "packages/web/pnpm-lock.yaml",
            "services/api/pom.xml",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        let commands = from_report_paths(
            &serde_json::json!({}),
            &paths,
            &[
                "packages/web/src/app.ts".into(),
                "services/api/src/Main.java".into(),
            ],
            &["./scripts/verify-contracts.sh".into()],
        );
        assert!(commands.contains(&"cargo test --all-targets".to_string()));
        assert!(commands.contains(&"cd packages/web && pnpm test".to_string()));
        assert!(commands.contains(&"cd services/api && mvn test".to_string()));
        assert!(commands.contains(&"./scripts/verify-contracts.sh".to_string()));
    }

    #[test]
    fn configured_command_families_replace_generic_autodetection() {
        let paths = ["Cargo.toml"]
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let commands = from_report_paths(
            &serde_json::json!({}),
            &paths,
            &["src/contract.rs".into()],
            &[
                "cargo fmt --all -- --check".into(),
                "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings"
                    .into(),
                "cargo test --workspace --all-targets --all-features --locked".into(),
            ],
        );
        assert_eq!(commands.len(), 3);
        assert!(
            !commands
                .iter()
                .any(|command| command == "cargo test contract")
        );
        assert!(
            !commands
                .iter()
                .any(|command| command == "cargo test --all-targets")
        );
    }
}
