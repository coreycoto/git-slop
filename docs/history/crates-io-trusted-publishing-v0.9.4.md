# crates.io trusted-publishing migration at v0.9.4

This document preserves the completed Issue #69 migration record. It is
historical evidence, not an active release checklist.

The v0.9.4 release was reserved for the first OIDC-backed crates.io
publication. Closeout recorded the successful Release Publish run ID and exact
source revision, then required the crates.io version API to attribute
publication to the same GitHub repository, run, and revision:

```bash
release_version=0.9.4
release_revision=<40-character-release-revision>
release_run_id=<release-publish-run-id>
curl --fail --silent --show-error \
  --user-agent "git-slop-release-history/1 (https://github.com/coreycoto/git-slop)" \
  "https://crates.io/api/v1/crates/git-slop/${release_version}" \
  | jq -e \
    --arg version "$release_version" \
    --arg revision "$release_revision" \
    --arg run_id "$release_run_id" \
    '.version.num == $version
     and .version.trustpub_data.provider == "github"
     and .version.trustpub_data.repository == "coreycoto/git-slop"
     and .version.trustpub_data.sha == $revision
     and .version.trustpub_data.run_id == $run_id'
```

Only after the exact tag, package checksum, draft and published GitHub Release,
Marketplace Action smoke, Homebrew Formula, consumer install, and
post-publication gates were terminal did the migration enable **Require trusted
publishing for all new versions**, revoke the old crates.io API token, and
remove the inert `CARGO_REGISTRY_TOKEN` environment secret. Current releases
must use the active checklist and must not recreate that rollback credential.
