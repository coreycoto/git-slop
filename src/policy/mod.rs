mod lifecycle;
mod model;
mod store;
mod validate;

pub use lifecycle::{
    init_pack, install_pack, list_packs, lock_selected_packs, remove_pack, resolve_for_advice,
    show_pack_or_rule, test_pack, validate_pack_reference,
};
pub use model::{CompiledPolicySet, ResolvedPack, Verdict, aggregate_verdict};
pub(crate) use store::policy_home;

pub const CORE_PACK_ID: &str = "org.git-slop.core";
pub const POLICY_SCHEMA_VERSION: u64 = 1;
pub const POLICY_LOCK_SCHEMA_VERSION: u64 = 1;
