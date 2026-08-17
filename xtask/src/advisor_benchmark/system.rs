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
    let physical_memory_bytes = system_physical_memory_bytes().or_else(|| {
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

fn system_physical_memory_bytes() -> Option<u64> {
    if cfg!(target_os = "macos") {
        return command_text("sysctl", &["-n", "hw.memsize"])
            .and_then(|value| value.parse::<u64>().ok());
    }
    fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find(|line| line.starts_with("MemTotal:"))?
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|kilobytes| kilobytes.saturating_mul(1024))
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
                    "Pages free" | "Pages inactive" | "Pages speculative"
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
        "provider_http_invalid",
        "provider_http_unsupported",
        "provider_timeout",
        "provider_unavailable",
        "provider_http_error",
        "provider_model_identity_missing",
        "provider_model_mismatch",
        "provider_completion_state_missing",
        "provider_incomplete_response",
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
                | "provider_http_invalid"
                | "provider_http_unsupported"
                | "provider_timeout"
                | "provider_unavailable"
                | "provider_http_error"
        )
    )
}

fn is_terminal_provider_identity_failure(category: Option<&str>) -> bool {
    matches!(
        category,
        Some("provider_model_identity_missing" | "provider_model_mismatch")
    )
}

#[derive(Debug, Clone, Copy)]
struct BenchmarkWatchdog {
    minimum_available_memory_bytes: u64,
    maximum_swap_growth_bytes: u64,
    initial_swap_used_bytes: u64,
}

struct MonitoredOutput {
    output: Output,
    peak_process_rss_bytes: Option<u64>,
    minimum_available_memory_bytes: Option<u64>,
    maximum_swap_growth_bytes: Option<u64>,
    termination_reason: Option<&'static str>,
}

struct BoundedRead {
    bytes: Vec<u8>,
    truncated: bool,
}

fn drain_bounded<R: Read>(
    mut reader: R,
    limit: usize,
    output_limit_exceeded: Arc<AtomicBool>,
) -> std::io::Result<BoundedRead> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut truncated = false;
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let retained = remaining.min(read);
        bytes.extend_from_slice(&chunk[..retained]);
        if retained < read {
            truncated = true;
            output_limit_exceeded.store(true, Ordering::Release);
        }
    }
    Ok(BoundedRead { bytes, truncated })
}

fn process_rss_bytes(pid: u32) -> Option<u64> {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().parse::<u64>().ok())
        .flatten()
        .map(|kilobytes| kilobytes.saturating_mul(1024))
}

fn timed_output(
    binary: &Path,
    args: &[String],
    cwd: &Path,
    watchdog: BenchmarkWatchdog,
) -> Result<MonitoredOutput> {
    let mut child = Command::new(binary);
    child
        .env("GIT_SLOP_ADVISOR_BENCHMARK", "1")
        .current_dir(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = child.spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("benchmark child stdout pipe is unavailable"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("benchmark child stderr pipe is unavailable"))?;
    let output_limit_exceeded = Arc::new(AtomicBool::new(false));
    let stdout_limit = Arc::clone(&output_limit_exceeded);
    let stderr_limit = Arc::clone(&output_limit_exceeded);
    let stdout_reader = thread::spawn(move || {
        drain_bounded(
            &mut stdout,
            BENCHMARK_CHILD_OUTPUT_LIMIT_BYTES,
            stdout_limit,
        )
    });
    let stderr_reader = thread::spawn(move || {
        drain_bounded(
            &mut stderr,
            BENCHMARK_CHILD_OUTPUT_LIMIT_BYTES,
            stderr_limit,
        )
    });
    let mut peak_rss = None::<u64>;
    let mut minimum_available = None::<u64>;
    let mut maximum_swap_growth = None::<u64>;
    let mut termination_reason = None;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if output_limit_exceeded.load(Ordering::Acquire) {
            termination_reason = Some("benchmark_child_output_limit");
        }
        if let Some(rss) = process_rss_bytes(child.id()) {
            peak_rss = Some(peak_rss.map_or(rss, |current| current.max(rss)));
        }
        if let Some(available) = system_available_memory_bytes() {
            minimum_available = Some(
                minimum_available.map_or(available, |current| current.min(available)),
            );
            if available < watchdog.minimum_available_memory_bytes {
                termination_reason.get_or_insert("resource_guard_available_memory");
            }
        } else {
            termination_reason.get_or_insert("resource_guard_measurement_unavailable");
        }
        if let Some(swap) = swap_used_bytes() {
            let growth = swap.saturating_sub(watchdog.initial_swap_used_bytes);
            maximum_swap_growth = Some(
                maximum_swap_growth.map_or(growth, |current| current.max(growth)),
            );
            if growth > watchdog.maximum_swap_growth_bytes {
                termination_reason.get_or_insert("resource_guard_swap_growth");
            }
        } else {
            termination_reason.get_or_insert("resource_guard_measurement_unavailable");
        }
        if termination_reason.is_some() {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    let status = child.wait()?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("benchmark stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("benchmark stderr reader panicked"))??;
    if (stdout.truncated || stderr.truncated) && termination_reason.is_none() {
        termination_reason = Some("benchmark_child_output_limit");
    }
    let mut output = Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    };
    if let Some(reason) = termination_reason {
        output.stderr.extend_from_slice(
            format!("\nadvisor benchmark aborted by safety guard {reason}; provider connection closed\n")
                .as_bytes(),
        );
    }
    Ok(MonitoredOutput {
        output,
        peak_process_rss_bytes: peak_rss,
        minimum_available_memory_bytes: minimum_available,
        maximum_swap_growth_bytes: maximum_swap_growth,
        termination_reason,
    })
}
