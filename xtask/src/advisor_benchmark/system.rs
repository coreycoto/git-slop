use super::*;

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = command_output_bounded(
        Command::new(program).args(args),
        64 * 1024,
        "system profile command",
    )
    .ok()?;
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
    let output = command_output_bounded(
        Command::new("system_profiler").args(["SPHardwareDataType", "-json"]),
        1024 * 1024,
        "macOS hardware profile",
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .ok()?
        .pointer("/SPHardwareDataType/0")
        .cloned()
}

pub(super) fn system_profile() -> Value {
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

pub(super) fn system_physical_memory_bytes() -> Option<u64> {
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

pub(super) fn parse_size(value: &str) -> Option<u64> {
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

pub(super) fn swap_used_bytes() -> Option<u64> {
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

pub(super) fn system_available_memory_bytes() -> Option<u64> {
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
                matches!(name, "Pages free" | "Pages inactive" | "Pages speculative")
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

fn structured_child_error_code(stderr: &[u8]) -> Option<String> {
    let diagnostic = String::from_utf8_lossy(stderr);
    diagnostic.lines().rev().find_map(|line| {
        serde_json::from_str::<Value>(line)
            .ok()?
            .pointer("/error/code")?
            .as_str()
            .map(str::to_string)
    })
}

pub(super) fn classify_failure(stderr: &[u8], artifact_present: bool) -> String {
    let code = structured_child_error_code(stderr);
    let allowed = [
        "provider_response_too_large",
        "provider_endpoint_invalid",
        "provider_endpoint_unsupported",
        "provider_remote_unsupported",
        "provider_operation_failed",
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
    ];
    if let Some(code) = code.filter(|code| allowed.contains(&code.as_str())) {
        return code;
    }
    if artifact_present {
        "artifact_invalid".to_string()
    } else {
        "artifact_unavailable".to_string()
    }
}

pub(super) fn is_provider_runtime_failure(category: Option<&str>) -> bool {
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

pub(super) fn is_terminal_provider_identity_failure(category: Option<&str>) -> bool {
    matches!(
        category,
        Some("provider_model_identity_missing" | "provider_model_mismatch")
    )
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BenchmarkWatchdog {
    pub(super) minimum_available_memory_bytes: u64,
    pub(super) maximum_swap_growth_bytes: u64,
    pub(super) initial_swap_used_bytes: u64,
}

pub(super) struct MonitoredOutput {
    pub(super) output: Output,
    pub(super) peak_process_rss_bytes: Option<u64>,
    pub(super) minimum_available_memory_bytes: Option<u64>,
    pub(super) maximum_swap_growth_bytes: Option<u64>,
    pub(super) termination_reason: Option<&'static str>,
}

pub(super) struct BoundedRead {
    pub(super) bytes: Vec<u8>,
    pub(super) truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ChildExecutionLimits {
    pub(super) output_limit_bytes: usize,
    pub(super) deadline: Duration,
    pub(super) poll_interval: Duration,
    pub(super) resource_monitor_stall_deadline: Option<Duration>,
    pub(super) require_resource_measurements: bool,
}

impl ChildExecutionLimits {
    fn production() -> Self {
        Self {
            output_limit_bytes: BENCHMARK_CHILD_OUTPUT_LIMIT_BYTES,
            deadline: Duration::from_secs(BENCHMARK_CHILD_DEADLINE_SECONDS),
            poll_interval: Duration::from_millis(250),
            resource_monitor_stall_deadline: Some(Duration::from_secs(2)),
            require_resource_measurements: true,
        }
    }
}

static INTERRUPT_HANDLER: Once = Once::new();
static BENCHMARK_INTERRUPTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn benchmark_signal_handler(_signal: libc::c_int) {
    BENCHMARK_INTERRUPTED.store(true, Ordering::Release);
}

fn install_interrupt_handler() {
    BENCHMARK_INTERRUPTED.store(false, Ordering::Release);
    #[cfg(unix)]
    INTERRUPT_HANDLER.call_once(|| unsafe {
        libc::signal(
            libc::SIGINT,
            benchmark_signal_handler as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            benchmark_signal_handler as *const () as libc::sighandler_t,
        );
    });
}

struct ChildGuard {
    child: Child,
    reaped: bool,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self {
            child,
            reaped: false,
        }
    }

    fn kill_group(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            unsafe {
                let _ = libc::kill(-(self.child.id() as libc::pid_t), libc::SIGTERM);
            }
            let grace_started = Instant::now();
            while grace_started.elapsed() < Duration::from_millis(500) {
                if self.child.try_wait()?.is_some() {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
        match self.child.kill() {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        Ok(self.child.try_wait()?)
    }

    fn wait(&mut self) -> Result<ExitStatus> {
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.kill_group();
            let _ = self.child.wait();
            self.reaped = true;
        }
    }
}

#[derive(Default)]
struct ResourceSnapshot {
    peak_rss: Option<u64>,
    minimum_available: Option<u64>,
    maximum_swap_growth: Option<u64>,
    measurement_failed: bool,
}

struct ResourceMonitor {
    snapshot: Arc<Mutex<ResourceSnapshot>>,
    heartbeat_ms: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl ResourceMonitor {
    fn start(pid: u32, initial_swap: u64) -> Self {
        let snapshot = Arc::new(Mutex::new(ResourceSnapshot::default()));
        let heartbeat_ms = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_snapshot = Arc::clone(&snapshot);
        let thread_heartbeat = Arc::clone(&heartbeat_ms);
        let thread_stop = Arc::clone(&stop);
        thread::spawn(move || {
            let started = Instant::now();
            while !thread_stop.load(Ordering::Acquire) {
                let rss = process_rss_bytes(pid);
                let available = system_available_memory_bytes();
                let swap = swap_used_bytes();
                if let Ok(mut state) = thread_snapshot.lock() {
                    state.measurement_failed |= available.is_none() || swap.is_none();
                    if let Some(rss) = rss {
                        state.peak_rss = Some(state.peak_rss.map_or(rss, |old| old.max(rss)));
                    }
                    if let Some(available) = available {
                        state.minimum_available = Some(
                            state
                                .minimum_available
                                .map_or(available, |old| old.min(available)),
                        );
                    }
                    if let Some(swap) = swap {
                        let growth = swap.saturating_sub(initial_swap);
                        state.maximum_swap_growth = Some(
                            state
                                .maximum_swap_growth
                                .map_or(growth, |old| old.max(growth)),
                        );
                    }
                }
                thread_heartbeat.store(
                    started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    Ordering::Release,
                );
                thread::sleep(Duration::from_millis(250));
            }
        });
        Self {
            snapshot,
            heartbeat_ms,
            stop,
        }
    }

    fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

pub(super) fn drain_bounded<R: Read>(
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
    let output = command_output_bounded(
        Command::new("ps").args(["-o", "rss=", "-p", &pid.to_string()]),
        64 * 1024,
        "process memory measurement",
    )
    .ok()?;
    output
        .status
        .success()
        .then(|| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u64>()
                .ok()
        })
        .flatten()
        .map(|kilobytes| kilobytes.saturating_mul(1024))
}

pub(super) fn timed_output(
    binary: &Path,
    args: &[String],
    cwd: &Path,
    watchdog: BenchmarkWatchdog,
) -> Result<MonitoredOutput> {
    timed_output_with_limits(
        binary,
        args,
        cwd,
        watchdog,
        ChildExecutionLimits::production(),
    )
}

pub(super) fn timed_output_with_limits(
    binary: &Path,
    args: &[String],
    cwd: &Path,
    watchdog: BenchmarkWatchdog,
    limits: ChildExecutionLimits,
) -> Result<MonitoredOutput> {
    if limits.output_limit_bytes == 0 || limits.deadline.is_zero() || limits.poll_interval.is_zero()
    {
        bail!("benchmark child execution limits must be positive");
    }
    install_interrupt_handler();
    let mut child = Command::new(binary);
    child
        .env("GIT_SLOP_ADVISOR_BENCHMARK", "1")
        .current_dir(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        child.process_group(0);
    }
    let mut child = ChildGuard::new(child.spawn()?);
    let mut stdout = child
        .child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("benchmark child stdout pipe is unavailable"))?;
    let mut stderr = child
        .child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("benchmark child stderr pipe is unavailable"))?;
    let output_limit_exceeded = Arc::new(AtomicBool::new(false));
    let stdout_limit = Arc::clone(&output_limit_exceeded);
    let stderr_limit = Arc::clone(&output_limit_exceeded);
    let output_limit_bytes = limits.output_limit_bytes;
    let stdout_reader =
        thread::spawn(move || drain_bounded(&mut stdout, output_limit_bytes, stdout_limit));
    let output_limit_bytes = limits.output_limit_bytes;
    let stderr_reader =
        thread::spawn(move || drain_bounded(&mut stderr, output_limit_bytes, stderr_limit));
    let monitor = ResourceMonitor::start(child.child.id(), watchdog.initial_swap_used_bytes);
    let started = Instant::now();
    let mut termination_reason = None;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if BENCHMARK_INTERRUPTED.load(Ordering::Acquire) {
            termination_reason = Some("operator_interrupt");
        }
        if started.elapsed() >= limits.deadline {
            termination_reason.get_or_insert("benchmark_child_deadline");
        }
        if output_limit_exceeded.load(Ordering::Acquire) {
            termination_reason = Some("benchmark_child_output_limit");
        }
        if let Some(stall_deadline) = limits.resource_monitor_stall_deadline {
            let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            let heartbeat = monitor.heartbeat_ms.load(Ordering::Acquire);
            let stall_ms = stall_deadline.as_millis().min(u64::MAX as u128) as u64;
            if elapsed_ms > stall_ms && elapsed_ms.saturating_sub(heartbeat) > stall_ms {
                termination_reason.get_or_insert("resource_guard_measurement_unavailable");
            }
        }
        if let Ok(state) = monitor.snapshot.lock() {
            if limits.require_resource_measurements && state.measurement_failed {
                termination_reason.get_or_insert("resource_guard_measurement_unavailable");
            }
            if state
                .minimum_available
                .is_some_and(|value| value < watchdog.minimum_available_memory_bytes)
            {
                termination_reason.get_or_insert("resource_guard_available_memory");
            }
            if state
                .maximum_swap_growth
                .is_some_and(|value| value > watchdog.maximum_swap_growth_bytes)
            {
                termination_reason.get_or_insert("resource_guard_swap_growth");
            }
        }
        if termination_reason.is_some() {
            child.kill_group()?;
            break;
        }
        thread::sleep(limits.poll_interval);
    }
    monitor.stop();
    let status = child.wait()?;
    let (peak_rss, minimum_available, maximum_swap_growth) = monitor
        .snapshot
        .lock()
        .map(|state| {
            (
                state.peak_rss,
                state.minimum_available,
                state.maximum_swap_growth,
            )
        })
        .unwrap_or((None, None, None));
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
            format!(
                "\nadvisor benchmark aborted by safety guard {reason}; provider connection closed\n"
            )
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
