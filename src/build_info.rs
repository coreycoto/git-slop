use serde::Serialize;

use crate::{PROJECT_NAME, VERSION};

pub const BUILD_INFO_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct BuildInfo {
    pub schema_version: u32,
    pub project: &'static str,
    pub version: &'static str,
    pub source_revision: Option<&'static str>,
    pub source_dirty: Option<bool>,
}

fn nonempty(value: &'static str) -> Option<&'static str> {
    (!value.is_empty()).then_some(value)
}

pub const fn from_embedded(
    source_revision: Option<&'static str>,
    source_dirty: Option<bool>,
) -> BuildInfo {
    BuildInfo {
        schema_version: BUILD_INFO_SCHEMA_VERSION,
        project: PROJECT_NAME,
        version: VERSION,
        source_revision,
        source_dirty,
    }
}

pub fn current() -> BuildInfo {
    from_embedded(
        nonempty(env!("GIT_SLOP_SOURCE_REVISION")),
        env!("GIT_SLOP_SOURCE_DIRTY").parse().ok(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_keeps_the_public_identity_shape() {
        let revision = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let info = from_embedded(Some(revision), Some(false));
        assert_eq!(info.schema_version, 1);
        assert_eq!(info.project, "git-slop");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.source_revision, Some(revision));
        assert_eq!(info.source_dirty, Some(false));
    }
}
