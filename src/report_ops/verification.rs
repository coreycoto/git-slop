use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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

fn discover(
    candidates: &[String],
    configured_commands: &[String],
    exists: impl Fn(&Path) -> bool + Copy,
    has_dotnet_project: impl Fn(&Path) -> bool + Copy,
) -> Vec<String> {
    let mut commands = Vec::new();
    for directory in candidate_directories(candidates) {
        let prefix = if directory.as_os_str().is_empty() {
            String::new()
        } else {
            format!("cd {} && ", directory.to_string_lossy())
        };
        for command in commands_for_directory(&directory, exists, has_dotnet_project) {
            let command = format!("{prefix}{command}");
            if !commands.contains(&command) {
                commands.push(command);
            }
        }
    }
    for command in configured_commands {
        if !command.trim().is_empty() && !commands.contains(command) {
            commands.push(command.clone());
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
    repository_paths: &BTreeSet<String>,
    candidates: &[String],
    configured_commands: &[String],
) -> Vec<String> {
    discover(
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
    )
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
}
