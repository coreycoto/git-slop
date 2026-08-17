const MAX_BENCHMARK_CONFIG_BYTES: usize = 4 * 1024 * 1024;
const MAX_BENCHMARK_RESULT_BYTES: usize = 64 * 1024 * 1024;
const MAX_BENCHMARK_REPORT_BYTES: usize = 256 * 1024 * 1024;
const MAX_BENCHMARK_CHILD_ARTIFACT_BYTES: usize = 8 * 1024 * 1024;
const BENCHMARK_AUXILIARY_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

fn read_bounded(path: &Path, maximum: usize, label: &str) -> Result<Vec<u8>> {
    let metadata = path
        .metadata()
        .with_context(|| format!("unable to inspect {label} {}", path.display()))?;
    if metadata.len() > maximum as u64 {
        bail!(
            "benchmark_input_too_large: {label} {} is {} bytes; maximum is {maximum} bytes",
            path.display(),
            metadata.len()
        );
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)?
        .take((maximum as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        bail!(
            "benchmark_input_too_large: {label} {} exceeds {maximum} bytes",
            path.display()
        );
    }
    Ok(bytes)
}

fn sha256_file(path: &Path, maximum: u64, label: &str) -> Result<String> {
    use sha2::{Digest, Sha256};

    let metadata = path.metadata()?;
    if metadata.len() > maximum {
        bail!(
            "benchmark_input_too_large: {label} {} is {} bytes; maximum is {maximum} bytes",
            path.display(),
            metadata.len()
        );
    }
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > maximum {
            bail!("benchmark_input_too_large: {label} exceeds {maximum} bytes");
        }
        digest.update(&chunk[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn command_output_bounded(
    command: &mut Command,
    maximum: usize,
    label: &str,
) -> Result<Output> {
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("{label} stdout is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("{label} stderr is unavailable"))?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let stdout_exceeded = Arc::clone(&exceeded);
    let stderr_exceeded = Arc::clone(&exceeded);
    let stdout_reader = thread::spawn(move || drain_bounded(stdout, maximum, stdout_exceeded));
    let stderr_reader = thread::spawn(move || drain_bounded(stderr, maximum, stderr_exceeded));
    let started = Instant::now();
    let mut timed_out = false;
    while child.try_wait()?.is_none() {
        if exceeded.load(Ordering::Acquire)
            || started.elapsed() >= BENCHMARK_AUXILIARY_COMMAND_TIMEOUT
        {
            timed_out = started.elapsed() >= BENCHMARK_AUXILIARY_COMMAND_TIMEOUT;
            match child.kill() {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
                Err(error) => return Err(error.into()),
            }
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let status = child.wait()?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("{label} stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("{label} stderr reader panicked"))??;
    if stdout.truncated || stderr.truncated || exceeded.load(Ordering::Acquire) {
        bail!("benchmark_output_too_large: {label} exceeded {maximum} bytes per stream");
    }
    if timed_out {
        bail!(
            "benchmark_command_timeout: {label} exceeded {} seconds",
            BENCHMARK_AUXILIARY_COMMAND_TIMEOUT.as_secs()
        );
    }
    Ok(Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}
