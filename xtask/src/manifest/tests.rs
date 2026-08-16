#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use tempfile::TempDir;

    use super::*;

    #[derive(Default)]
    struct FakeRunner {
        outputs: VecDeque<String>,
        output_calls: Vec<(PathBuf, CommandSpec)>,
        run_calls: Vec<(PathBuf, CommandSpec)>,
    }

    impl CommandRunner for FakeRunner {
        fn output(&mut self, cwd: &Path, command: &CommandSpec) -> Result<String> {
            self.output_calls.push((cwd.to_path_buf(), command.clone()));
            self.outputs
                .pop_front()
                .ok_or_else(|| anyhow!("missing fake output"))
        }

        fn run(&mut self, cwd: &Path, command: &CommandSpec) -> Result<()> {
            self.run_calls.push((cwd.to_path_buf(), command.clone()));
            Ok(())
        }
    }

    fn fixture() -> Result<(TempDir, PathBuf)> {
        let temp = tempfile::tempdir()?;
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"git-slop\"\nversion = \"0.9.0\"\n",
        )?;
        let dist = temp.path().join("dist");
        fs::create_dir(&dist)?;
        for target in RELEASE_TARGETS {
            fs::write(
                dist.join(artifact_name("v0.9.0", target)),
                format!("{}\n", target.target),
            )?;
        }
        Ok((temp, dist))
    }

    fn crate_source() -> CrateSource {
        CrateSource::new(
            "0.9.0",
            "f".repeat(64),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
    }

    include!("tests/group_1.rs");
    include!("tests/group_2.rs");
    include!("tests/group_3.rs");

}
