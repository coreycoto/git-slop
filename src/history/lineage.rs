fn apply_rename_aliases(aliases: &mut BTreeMap<String, String>, commit: Option<&StatusCommit>) {
    let Some(commit) = commit else {
        return;
    };
    for change in &commit.changes {
        let StatusChange::Rename { old_path, new_path } = change else {
            continue;
        };
        if let Some(current_path) = aliases.get(new_path).cloned() {
            aliases.insert(old_path.clone(), current_path);
        }
    }
}

fn mapped_paths_for_status_change(
    change: &StatusChange,
    aliases: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    match change {
        StatusChange::Rename { old_path, new_path } => {
            if let Some(current) = aliases.get(new_path).or_else(|| aliases.get(old_path)) {
                result.insert(current.clone());
            }
        }
        StatusChange::Copy { new_path, .. } => {
            if let Some(current) = aliases.get(new_path) {
                result.insert(current.clone());
            }
        }
        StatusChange::Path { path, .. } => {
            if let Some(current) = aliases.get(path) {
                result.insert(current.clone());
            }
        }
    }
    result
}

fn first_seen_exact(
    tracked_paths: &BTreeSet<String>,
    commits: &[StatusCommit],
) -> BTreeMap<String, Option<i64>> {
    let mut appearances = BTreeMap::new();
    let mut fallbacks = BTreeMap::new();
    for commit in commits {
        for change in &commit.changes {
            match change {
                StatusChange::Rename { new_path, .. } if tracked_paths.contains(new_path) => {
                    appearances.insert(new_path.clone(), commit.timestamp);
                    fallbacks.insert(new_path.clone(), commit.timestamp);
                }
                StatusChange::Path { status, path } if tracked_paths.contains(path) => {
                    fallbacks.insert(path.clone(), commit.timestamp);
                    if status.starts_with('A') {
                        appearances.insert(path.clone(), commit.timestamp);
                    }
                }
                _ => {}
            }
        }
    }
    tracked_paths
        .iter()
        .map(|path| {
            (
                path.clone(),
                appearances
                    .get(path)
                    .copied()
                    .or_else(|| fallbacks.get(path).copied()),
            )
        })
        .collect()
}

fn first_seen_with_lineage(
    tracked_paths: &BTreeSet<String>,
    commits: &[StatusCommit],
) -> BTreeMap<String, Option<i64>> {
    let mut aliases: BTreeMap<String, String> = tracked_paths
        .iter()
        .map(|path| (path.clone(), path.clone()))
        .collect();
    let mut result: BTreeMap<String, Option<i64>> = tracked_paths
        .iter()
        .map(|path| (path.clone(), None))
        .collect();
    for commit in commits {
        let mut touched = BTreeSet::new();
        for change in &commit.changes {
            touched.extend(mapped_paths_for_status_change(change, &aliases));
        }
        for path in touched {
            result.insert(path, Some(commit.timestamp));
        }
        apply_rename_aliases(&mut aliases, Some(commit));
    }
    result
}

fn map_numstat_exact(entry: &NumstatEntry, tracked_paths: &BTreeSet<String>) -> Option<String> {
    match entry.paths.as_slice() {
        [old_path, new_path, ..] => {
            if tracked_paths.contains(new_path) {
                Some(new_path.clone())
            } else if tracked_paths.contains(old_path) {
                Some(old_path.clone())
            } else {
                None
            }
        }
        [path] if tracked_paths.contains(path) => Some(path.clone()),
        _ => None,
    }
}

fn map_numstat_with_lineage(
    entry: &NumstatEntry,
    aliases: &BTreeMap<String, String>,
) -> Option<String> {
    match entry.paths.as_slice() {
        [old_path, new_path, ..] => aliases
            .get(new_path)
            .or_else(|| aliases.get(old_path))
            .cloned(),
        [path] => aliases.get(path).cloned(),
        _ => None,
    }
}
