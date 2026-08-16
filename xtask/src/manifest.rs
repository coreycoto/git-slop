use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::crates_io::CrateSource;

mod install;

use install::install_instructions;

pub const PROJECT_NAME: &str = "git-slop";
pub const REPO_FULL_NAME: &str = "coreycoto/git-slop";
pub const MANIFEST_SCHEMA_VERSION: u32 = 3;
pub const CHECKSUM_FILE_NAME: &str = "SHA256SUMS";
pub const SUPPLEMENTAL_RELEASE_ASSETS: [(&str, &str, &str); 3] = [
    ("git-slop.rb", "homebrew_formula", "text/x-ruby"),
    (
        "git-slop.cdx.json",
        "cyclonedx_sbom",
        "application/vnd.cyclonedx+json",
    ),
    ("git-slop.spdx.json", "spdx_sbom", "application/spdx+json"),
];
/// Public Action download and manifest limit for every native release archive.
pub const MAX_RELEASE_ARTIFACT_BYTES: u64 = 128 * 1024 * 1024;

include!("manifest/model.rs");
include!("manifest/identity.rs");
include!("manifest/build.rs");
include!("manifest/output.rs");
include!("manifest/tests.rs");
