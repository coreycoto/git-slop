use serde::Serialize;

use crate::{PROJECT_NAME, VERSION};

pub const BUILD_INFO_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct BuildInfo {
    pub schema_version: u32,
    pub project: &'static str,
    pub version: &'static str,
    pub source_revision: Option<&'static str>,
    pub source_dirty: Option<bool>,
    pub target: &'static str,
    pub crate_sha256: Option<&'static str>,
    pub rustc_version: &'static str,
    pub build_source: &'static str,
}

fn nonempty(value: &'static str) -> Option<&'static str> {
    (!value.is_empty()).then_some(value)
}

pub const fn from_embedded(
    source_revision: Option<&'static str>,
    source_dirty: Option<bool>,
    target: &'static str,
    crate_sha256: Option<&'static str>,
    rustc_version: &'static str,
    build_source: &'static str,
) -> BuildInfo {
    BuildInfo {
        schema_version: BUILD_INFO_SCHEMA_VERSION,
        project: PROJECT_NAME,
        version: VERSION,
        source_revision,
        source_dirty,
        target,
        crate_sha256,
        rustc_version,
        build_source,
    }
}

pub fn current() -> BuildInfo {
    from_embedded(
        nonempty(env!("GIT_SLOP_SOURCE_REVISION")),
        env!("GIT_SLOP_SOURCE_DIRTY").parse().ok(),
        env!("GIT_SLOP_BUILD_TARGET"),
        nonempty(env!("GIT_SLOP_CRATE_SHA256")),
        env!("GIT_SLOP_RUSTC_VERSION"),
        env!("GIT_SLOP_BUILD_SOURCE"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_keeps_the_public_identity_shape() {
        let revision = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let info = from_embedded(
            Some(revision),
            Some(false),
            "x86_64-unknown-linux-gnu",
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "rustc 1.97.1",
            "release",
        );
        assert_eq!(info.schema_version, 2);
        assert_eq!(info.project, "git-slop");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.source_revision, Some(revision));
        assert_eq!(info.source_dirty, Some(false));
        assert_eq!(info.target, "x86_64-unknown-linux-gnu");
        assert_eq!(info.build_source, "release");
    }
}
