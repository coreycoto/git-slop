fn git_has_head(repo_root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .context("failed to execute git")?;
    Ok(output.status.success())
}

fn stream_git_log<T>(
    repo_root: &Path,
    args: &[String],
    parse: fn(&str) -> Vec<T>,
) -> Result<Vec<T>> {
    let mut child = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to execute git")?;
    let stdout = child
        .stdout
        .take()
        .context("git log stdout was unavailable")?;
    let mut reader = std::io::BufReader::new(stdout);
    let mut token = Vec::new();
    let mut commit = String::new();
    let mut records = Vec::new();
    loop {
        token.clear();
        let bytes = reader.read_until(0, &mut token)?;
        if bytes == 0 {
            break;
        }
        if token.last() == Some(&0) {
            token.pop();
        }
        let value = String::from_utf8_lossy(&token);
        if value.trim_start_matches('\n') == "commit" && !commit.is_empty() {
            records.extend(parse(&commit));
            commit.clear();
        }
        commit.push_str(&value);
        commit.push('\0');
    }
    if !commit.is_empty() {
        records.extend(parse(&commit));
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "git {} failed{}",
            args.join(" "),
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }
    Ok(records)
}

fn load_status_commits(
    repo_root: &Path,
    since: Option<&str>,
    follow_renames: bool,
    max_commits: u64,
) -> Result<Vec<StatusCommit>> {
    let mut args = vec![
        "log".to_string(),
        "--no-show-signature".to_string(),
        "--name-status".to_string(),
        "-z".to_string(),
        "--format=commit%x00%H%x00%ct%x00%an%x00%ae%x00%P%x00%s".to_string(),
        if follow_renames {
            "--find-renames".to_string()
        } else {
            "--no-renames".to_string()
        },
        format!("--max-count={max_commits}"),
    ];
    if let Some(since) = since {
        args.push(format!("--since={since}"));
    }
    stream_git_log(repo_root, &args, parse_status_log)
}

fn load_numstat_commits(
    repo_root: &Path,
    since: &str,
    follow_renames: bool,
    max_commits: u64,
) -> Result<Vec<NumstatCommit>> {
    let args = vec![
        "log".to_string(),
        "--no-show-signature".to_string(),
        "--numstat".to_string(),
        "-z".to_string(),
        "--format=commit%x00%H%x00%ct%x00%an%x00%ae%x00%P%x00%s".to_string(),
        format!("--since={since}"),
        format!("--max-count={max_commits}"),
        if follow_renames {
            "--find-renames".to_string()
        } else {
            "--no-renames".to_string()
        },
    ];
    // Preserve the established ordering contract: commits are emitted newest
    // first and rename aliases are expanded while walking back.
    stream_git_log(repo_root, &args, parse_numstat_log)
}
