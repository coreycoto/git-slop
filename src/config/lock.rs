use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{Context, Result};
use fs2::FileExt;
use serde_json::json;

use crate::error::{ClassifiedError, ErrorKind};

#[derive(Debug)]
pub struct ScanLock {
    file: File,
}

impl Drop for ScanLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn acquire_scan_lock(state_root: &Path) -> Result<ScanLock> {
    fs::create_dir_all(state_root).with_context(|| {
        format!(
            "failed to create Git-private runtime directory {}",
            state_root.display()
        )
    })?;
    let path = state_root.join("scan.lock");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("failed to open scan lock {}", path.display()))?;
    if file.try_lock_exclusive().is_err() {
        file.seek(SeekFrom::Start(0))?;
        let mut owner = String::new();
        file.read_to_string(&mut owner)?;
        let pid = owner
            .trim()
            .strip_prefix("pid=")
            .and_then(|value| value.parse::<u32>().ok());
        return Err(ClassifiedError::new(
            ErrorKind::ResourceLimit,
            "scan_locked",
            format!("another git-slop scan owns {}", path.display()),
        )
        .at("/state_dir/scan.lock")
        .with_details(json!({
            "lock_path": path,
            "owner_pid": pid,
            "retry_guidance": "wait for the owning scan or choose a distinct --state-dir"
        }))
        .into());
    }
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    writeln!(file, "pid={}", std::process::id())?;
    file.sync_data()?;
    Ok(ScanLock { file })
}
