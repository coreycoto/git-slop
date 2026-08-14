use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisEstimate {
    pub tracked_path_count: usize,
    pub inventory_bytes: u128,
    pub estimated_peak_memory_bytes: u128,
    pub estimated_peak_memory_low_bytes: u128,
    pub estimated_peak_memory_high_bytes: u128,
    pub runtime_overhead_bytes: u128,
    pub confidence: String,
    pub memory_budget_bytes: u128,
    pub estimated_cache_bytes: u128,
    pub estimated_report_bytes: u128,
    pub estimated_relationship_count: u128,
    pub estimated_inode_count: u128,
    pub estimated_seconds: u128,
    pub estimated_history_commit_count: u64,
    pub path_breadth: usize,
    pub dominant_subsystem: Option<String>,
    pub symlink_count: usize,
}

/// Return the process resident-set size in bytes when the host exposes it.
///
/// Linux reports the current RSS through procfs. macOS exposes the process
/// high-water RSS through `getrusage`. Unsupported hosts deliberately return
/// `None` rather than inventing a measurement.
pub fn current_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        let kib = status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?;
        kib.checked_mul(1024)
    }
    #[cfg(target_os = "macos")]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        // SAFETY: getrusage initializes the supplied rusage buffer for the
        // current process and does not retain its pointer.
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
            return None;
        }
        // macOS reports ru_maxrss in bytes (Linux reports KiB, but Linux uses
        // current VmRSS from procfs above).
        u64::try_from(unsafe { usage.assume_init() }.ru_maxrss).ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

pub fn build(repo_root: &Path, paths: &[String], config_value: &Value) -> AnalysisEstimate {
    let mut subsystem_bytes = BTreeMap::<String, u128>::new();
    let mut symlink_count = 0usize;
    let inventory_bytes = paths
        .iter()
        .map(|path| {
            let absolute = repo_root.join(path);
            let bytes = fs::symlink_metadata(&absolute).ok().map_or(0, |metadata| {
                if metadata.file_type().is_symlink() {
                    symlink_count += 1;
                    fs::read_link(&absolute)
                        .ok()
                        .map(|target| target.as_os_str().len() as u128)
                        .unwrap_or_default()
                } else {
                    metadata.len() as u128
                }
            });
            let subsystem = path.split('/').next().unwrap_or(".").to_string();
            *subsystem_bytes.entry(subsystem).or_default() += bytes;
            bytes
        })
        .sum::<u128>();
    let dominant_subsystem = subsystem_bytes
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(path, _)| path);
    let estimated_history_commit_count = Command::new("git")
        .current_dir(repo_root)
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or_default();
    let path_count = paths.len() as u128;
    let pair_limit =
        config::pointer_u64(config_value, "/organization/max_pairs_per_file", 20) as u128;
    let graph_bytes = path_count.saturating_mul(pair_limit).saturating_mul(256);
    let history_bytes = path_count
        .saturating_mul(1_024)
        .saturating_add(estimated_history_commit_count as u128 * 384);
    let tokenizer_bytes = inventory_bytes.saturating_mul(2);
    let report_bytes = inventory_bytes
        .saturating_div(2)
        .saturating_add(path_count.saturating_mul(4_096));
    // The tokenizer, SQLite, Git subprocesses, and report assembly have a
    // meaningful fixed floor even for small repositories. Keep the estimate
    // deliberately conservative until repository-run telemetry is available.
    let runtime_overhead_bytes = 64_u128 * 1024 * 1024;
    let estimated_peak_memory_bytes = inventory_bytes
        .saturating_add(tokenizer_bytes)
        .saturating_add(graph_bytes)
        .saturating_add(history_bytes)
        .saturating_add(report_bytes)
        .saturating_add(runtime_overhead_bytes);
    AnalysisEstimate {
        tracked_path_count: paths.len(),
        inventory_bytes,
        estimated_peak_memory_bytes,
        estimated_peak_memory_low_bytes: estimated_peak_memory_bytes.saturating_mul(75) / 100,
        estimated_peak_memory_high_bytes: estimated_peak_memory_bytes.saturating_mul(175) / 100,
        runtime_overhead_bytes,
        confidence: if paths.is_empty() {
            "low"
        } else if estimated_history_commit_count == 0 {
            "moderate"
        } else {
            "calibrated_heuristic"
        }
        .to_string(),
        memory_budget_bytes: config::pointer_u64(config_value, "/resources/memory_budget_mb", 1024)
            as u128
            * 1024
            * 1024,
        estimated_cache_bytes: inventory_bytes
            .saturating_div(3)
            .saturating_add(path_count * 512),
        estimated_report_bytes: report_bytes,
        estimated_relationship_count: path_count.saturating_mul(pair_limit).saturating_div(2),
        estimated_inode_count: path_count.saturating_add(16),
        estimated_seconds: inventory_bytes
            .div_ceil(4 * 1024 * 1024)
            .saturating_add(path_count.div_ceil(2_000))
            .saturating_add((estimated_history_commit_count as u128).div_ceil(5_000))
            .max(6),
        estimated_history_commit_count,
        path_breadth: paths
            .iter()
            .map(|path| path.split('/').next().unwrap_or("."))
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        dominant_subsystem,
        symlink_count,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use tempfile::tempdir;

    use super::{build, current_rss_bytes};
    use crate::config;

    #[test]
    fn thirty_thousand_path_preflight_is_bounded_and_reports_breadth() {
        let repository = tempdir().unwrap();
        let paths = (0..30_000)
            .map(|index| format!("package-{}/src/file-{index}.rs", index % 30))
            .collect::<Vec<_>>();
        let started = Instant::now();
        let estimate = build(repository.path(), &paths, &config::default_config());
        assert_eq!(estimate.tracked_path_count, 30_000);
        assert_eq!(estimate.path_breadth, 30);
        assert!(estimate.estimated_peak_memory_bytes > 0);
        assert!(estimate.estimated_report_bytes > 0);
        assert!(
            started.elapsed().as_secs() < 10,
            "30k preflight exceeded 10 seconds"
        );
    }

    #[test]
    fn one_thousand_and_one_hundred_thousand_path_fixtures_scale_monotonically() {
        let repository = tempdir().unwrap();
        let estimate_for = |count| {
            let paths = (0..count)
                .map(|index| format!("package-{}/src/file-{index}.rs", index % 100))
                .collect::<Vec<_>>();
            build(repository.path(), &paths, &config::default_config())
        };
        let small = estimate_for(1_000);
        let large = estimate_for(100_000);
        assert_eq!(small.tracked_path_count, 1_000);
        assert_eq!(large.tracked_path_count, 100_000);
        assert!(large.estimated_peak_memory_low_bytes > small.estimated_peak_memory_high_bytes);
        assert!(large.estimated_seconds >= small.estimated_seconds);
        assert_eq!(large.path_breadth, 100);
    }

    #[test]
    fn representative_small_repository_estimate_keeps_a_conservative_range() {
        let repository = tempdir().unwrap();
        let paths = (0..291)
            .map(|index| format!("src/file-{index}.rs"))
            .collect::<Vec<_>>();
        let estimate = build(repository.path(), &paths, &config::default_config());

        // The audit regression that motivated this fixture observed an
        // approximately 88 MiB first-run peak and a scan longer than six
        // seconds at this path count. Keep those observations inside the
        // advertised conservative envelope without claiming benchmark-grade
        // calibration.
        assert!(estimate.estimated_peak_memory_high_bytes >= 88 * 1024 * 1024);
        assert!(estimate.estimated_seconds >= 6);
        assert!(
            estimate.estimated_peak_memory_low_bytes < estimate.estimated_peak_memory_high_bytes
        );
    }

    #[test]
    fn preflight_performance_contract_gates_wall_rss_report_cache_and_relationships() {
        let repository = tempdir().unwrap();
        let started_rss = current_rss_bytes().unwrap_or_default();
        for (count, maximum_seconds) in [(1_000usize, 2u64), (30_000, 10), (100_000, 20)] {
            let paths = (0..count)
                .map(|index| format!("package-{}/src/file-{index}.rs", index % 100))
                .collect::<Vec<_>>();
            let started = Instant::now();
            let estimate = build(repository.path(), &paths, &config::default_config());
            assert!(
                started.elapsed().as_secs() < maximum_seconds,
                "{count}-path preflight exceeded {maximum_seconds} seconds"
            );
            assert!(estimate.estimated_report_bytes <= count as u128 * 4_096);
            assert!(estimate.estimated_cache_bytes <= count as u128 * 512);
            assert!(estimate.estimated_relationship_count <= count as u128 * 10);
        }
        let peak_delta = current_rss_bytes()
            .unwrap_or(started_rss)
            .saturating_sub(started_rss);
        assert!(
            peak_delta <= 512 * 1024 * 1024,
            "synthetic preflight fixtures used {} MiB additional RSS",
            peak_delta.div_ceil(1024 * 1024)
        );
    }

    #[test]
    fn supported_hosts_expose_a_nonzero_rss_measurement() {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        assert!(current_rss_bytes().is_some_and(|bytes| bytes > 0));
    }
}
