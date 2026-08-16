fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .chars()
            .take(500)
            .collect()
    })
}

fn mac_hardware_profile() -> Option<Value> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = Command::new("system_profiler")
        .args(["SPHardwareDataType", "-json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .ok()?
        .pointer("/SPHardwareDataType/0")
        .cloned()
}

fn system_profile() -> Value {
    let mac = mac_hardware_profile();
    let hardware_model = command_text("sysctl", &["-n", "hw.model"]).or_else(|| {
        mac.as_ref()
            .and_then(|value| value.get("machine_model"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let physical_memory_bytes = command_text("sysctl", &["-n", "hw.memsize"])
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| {
            mac.as_ref()
                .and_then(|value| value.get("physical_memory"))
                .and_then(Value::as_str)
                .and_then(parse_size)
        });
    let cpu = command_text("sysctl", &["-n", "machdep.cpu.brand_string"]).or_else(|| {
        mac.as_ref()
            .and_then(|value| value.get("chip_type"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    json!({
        "architecture": command_text("uname", &["-m"]),
        "os_release": command_text("uname", &["-sr"]),
        "macos_version": command_text("sw_vers", &["-productVersion"]),
        "hardware_model": hardware_model,
        "physical_memory_bytes": physical_memory_bytes,
        "cpu": cpu,
        "privacy": "No username, home path, serial number, hardware UUID, repository path, source excerpt, prompt, or rationale is recorded."
    })
}

fn parse_size(value: &str) -> Option<u64> {
    let value = value.trim().trim_end_matches(',');
    let split = value
        .char_indices()
        .find(|(_, character)| !character.is_ascii_digit() && *character != '.')
        .map(|(index, _)| index)
        .unwrap_or(value.len());
    let number = value[..split].parse::<f64>().ok()?;
    let unit = value[split..].trim().to_ascii_uppercase();
    let multiplier = match unit.as_str() {
        "" | "B" => 1.0,
        "K" | "KB" => 1024.0,
        "M" | "MB" => 1024.0 * 1024.0,
        "G" | "GB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * multiplier) as u64)
}

fn swap_used_bytes() -> Option<u64> {
    if cfg!(target_os = "macos") {
        let output = command_text("sysctl", &["-n", "vm.swapusage"])?;
        let used = output
            .split_whitespace()
            .skip_while(|part| *part != "used")
            .nth(2)?;
        return parse_size(used);
    }
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
    let total = meminfo
        .lines()
        .find(|line| line.starts_with("SwapTotal:"))?
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()?
        * 1024;
    let free = meminfo
        .lines()
        .find(|line| line.starts_with("SwapFree:"))?
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()?
        * 1024;
    Some(total.saturating_sub(free))
}

fn system_available_memory_bytes() -> Option<u64> {
    if cfg!(target_os = "macos") {
        let output = command_text("vm_stat", &[])?;
        let page_size = output
            .lines()
            .next()?
            .split("page size of ")
            .nth(1)?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?;
        let available_pages = output
            .lines()
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                matches!(
                    name,
                    "Pages free"
                        | "Pages inactive"
                        | "Pages speculative"
                        | "Pages purgeable"
                )
                .then(|| value.trim().trim_end_matches('.').parse::<u64>().ok())
                .flatten()
            })
            .sum::<u64>();
        return (available_pages > 0).then_some(available_pages.saturating_mul(page_size));
    }
    fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find(|line| line.starts_with("MemAvailable:"))?
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|kilobytes| kilobytes.saturating_mul(1024))
}

fn classify_failure(stderr: &[u8], artifact_present: bool) -> String {
    let diagnostic = String::from_utf8_lossy(stderr);
    for category in [
        "provider_response_too_large",
        "provider_response_invalid",
        "provider_timeout",
        "provider_unavailable",
        "provider_http_error",
    ] {
        if diagnostic.contains(category) {
            return category.to_string();
        }
    }
    if artifact_present {
        "artifact_invalid".to_string()
    } else {
        "artifact_unavailable".to_string()
    }
}

fn is_provider_runtime_failure(category: Option<&str>) -> bool {
    matches!(
        category,
        Some(
            "provider_response_too_large"
                | "provider_timeout"
                | "provider_unavailable"
                | "provider_http_error"
        )
    )
}

fn timed_output(binary: &Path, args: &[String], cwd: &Path) -> Result<(Output, Option<u64>)> {
    let time = Path::new("/usr/bin/time");
    if !time.is_file() {
        return Ok((
            Command::new(binary)
                .env("GIT_SLOP_ADVISOR_BENCHMARK", "1")
                .current_dir(cwd)
                .args(args)
                .output()?,
            None,
        ));
    }
    let mut command = Command::new(time);
    command
        .env("GIT_SLOP_ADVISOR_BENCHMARK", "1")
        .current_dir(cwd);
    if cfg!(target_os = "macos") {
        command.arg("-l");
    } else {
        command.arg("-v");
    }
    let output = command.arg(binary).args(args).output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let rss = if cfg!(target_os = "macos") {
        stderr.lines().find_map(|line| {
            line.contains("maximum resident set size")
                .then(|| line.split_whitespace().next()?.parse::<u64>().ok())
                .flatten()
        })
    } else {
        stderr.lines().find_map(|line| {
            line.contains("Maximum resident set size")
                .then(|| {
                    line.split(':')
                        .nth(1)?
                        .trim()
                        .parse::<u64>()
                        .ok()
                        .map(|kb| kb * 1024)
                })
                .flatten()
        })
    };
    Ok((output, rss))
}
