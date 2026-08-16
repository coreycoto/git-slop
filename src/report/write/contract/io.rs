fn compressed_bytes(format: &str, source: &[u8]) -> Result<Option<(String, Vec<u8>)>> {
    match format {
        "none" => Ok(None),
        "gzip" => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(source)?;
            Ok(Some(("report.json.gz".to_string(), encoder.finish()?)))
        }
        "zstd" => Ok(Some((
            "report.json.zst".to_string(),
            zstd::stream::encode_all(source, 3)?,
        ))),
        value => anyhow::bail!("unsupported report compression {value:?}"),
    }
}

pub fn write_json_atomically(path: &Path, value: &Value) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let temporary = temporary_directory(parent, "report-migration");
    fs::write(&temporary, serde_json::to_string_pretty(value)? + "\n")
        .with_context(|| format!("failed to write {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("failed to publish {}", path.display()))
}
