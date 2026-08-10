fn normalized_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn author_key(name: &str, email: &str) -> String {
    let name = name.trim();
    let email = email.trim();
    format!("{name} <{email}>")
}

fn parse_status_log(raw: &str) -> Vec<StatusCommit> {
    let tokens: Vec<&str> = raw.split('\0').collect();
    let mut commits = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index] != "commit" || index + 4 >= tokens.len() {
            index += 1;
            continue;
        }
        let commit = tokens[index + 1].trim().to_string();
        let timestamp = tokens[index + 2].trim().parse::<i64>().unwrap_or(0);
        let author = author_key(tokens[index + 3], tokens[index + 4]);
        let extended = tokens.get(index + 5).is_some_and(|value| {
            value.is_empty()
                || value.split_whitespace().all(|parent| {
                    parent.len() == 40
                        && parent
                            .chars()
                            .all(|character| character.is_ascii_hexdigit())
                })
        });
        let parents = if extended {
            tokens[index + 5]
                .split_whitespace()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let subject = if extended {
            tokens.get(index + 6).unwrap_or(&"").to_string()
        } else {
            String::new()
        };
        index += if extended { 7 } else { 5 };
        let mut changes = Vec::new();
        while index < tokens.len() && tokens[index] != "commit" {
            let status = tokens[index].trim_start_matches('\n');
            index += 1;
            if status.is_empty() {
                continue;
            }
            match status.as_bytes()[0] {
                b'R' | b'C' => {
                    if index + 1 >= tokens.len() {
                        break;
                    }
                    let old_path = normalized_path(tokens[index]);
                    let new_path = normalized_path(tokens[index + 1]);
                    index += 2;
                    if old_path.is_empty() || new_path.is_empty() {
                        continue;
                    }
                    if status.starts_with('R') {
                        changes.push(StatusChange::Rename { old_path, new_path });
                    } else {
                        changes.push(StatusChange::Copy { old_path, new_path });
                    }
                }
                _ => {
                    if index >= tokens.len() {
                        break;
                    }
                    let path = normalized_path(tokens[index]);
                    index += 1;
                    if !path.is_empty() {
                        changes.push(StatusChange::Path {
                            status: status.to_string(),
                            path,
                        });
                    }
                }
            }
        }
        commits.push(StatusCommit {
            commit,
            timestamp,
            author,
            parents,
            subject,
            changes,
        });
    }
    commits
}

fn parse_numstat_log(raw: &str) -> Vec<NumstatCommit> {
    let tokens: Vec<&str> = raw.split('\0').collect();
    let mut commits = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index] != "commit" || index + 4 >= tokens.len() {
            index += 1;
            continue;
        }
        let commit = tokens[index + 1].trim().to_string();
        let timestamp = tokens[index + 2].trim().parse::<i64>().unwrap_or(0);
        let author = author_key(tokens[index + 3], tokens[index + 4]);
        let extended = tokens.get(index + 5).is_some_and(|value| {
            value.is_empty()
                || value.split_whitespace().all(|parent| {
                    parent.len() == 40
                        && parent
                            .chars()
                            .all(|character| character.is_ascii_hexdigit())
                })
        });
        let parents = if extended {
            tokens[index + 5]
                .split_whitespace()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let subject = if extended {
            tokens.get(index + 6).unwrap_or(&"").to_string()
        } else {
            String::new()
        };
        index += if extended { 7 } else { 5 };
        let mut entries = Vec::new();
        while index < tokens.len() && tokens[index] != "commit" {
            let stat_line = tokens[index].trim_start_matches('\n');
            index += 1;
            if stat_line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = stat_line.splitn(3, '\t').collect();
            if parts.len() < 2 {
                continue;
            }
            let mut paths = Vec::new();
            if parts.get(2).is_some_and(|path| !path.is_empty()) {
                paths.push(normalized_path(parts[2]));
            } else {
                if index >= tokens.len() || tokens[index] == "commit" {
                    break;
                }
                if !tokens[index].is_empty() {
                    paths.push(normalized_path(tokens[index]));
                }
                index += 1;
                if index < tokens.len() && tokens[index] != "commit" && !tokens[index].is_empty() {
                    paths.push(normalized_path(tokens[index]));
                    index += 1;
                }
            }
            if parts[0] == "-" || parts[1] == "-" {
                continue;
            }
            let (Ok(added), Ok(deleted)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>())
            else {
                continue;
            };
            entries.push(NumstatEntry {
                added,
                deleted,
                paths,
            });
        }
        commits.push(NumstatCommit {
            commit,
            timestamp,
            author,
            parents,
            subject,
            entries,
        });
    }
    commits
}
