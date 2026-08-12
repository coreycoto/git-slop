# Release trust graph

Git Slop release verification starts from two independent roots: the pinned
OpenPGP primary-key fingerprint for the annotated source tag, and GitHub's OIDC
identity for crates.io trusted publishing. Neither root is derived from a
release asset.

The signed tag binds the source revision. The crates.io index and static crate
API bind that revision to the immutable `.crate` digest. Seven native archives
are built from those exact crate bytes. `SHA256SUMS` and
`release-manifest.json` inventory the archive and supplemental-asset digests;
GitHub artifact attestations independently bind each uploaded subject name and
SHA-256 digest to the release workflow. The release manifest does not list its
own digest and therefore is not treated as a circular trust root.

Consumers should verify, in order:

1. the immutable release record with
   `gh release verify <tag> --repo coreycoto/git-slop` and the annotated tag
   against the pinned full fingerprint;
2. the crate checksum and embedded source revision;
3. the selected archive against `SHA256SUMS` and `release-manifest.json`;
4. the archive attestation subject and digest using the exact per-asset command
   listed in the schema-3 manifest; and
5. the installed binary's `build-info` revision, crate digest, target, and
   clean-source state.

Homebrew and Scoop are downstream projections. Their receiver workflows must
reverify the public immutable GitHub release identity and exact archive or crate
digest before publishing; they are not additional trust roots.

Published release-note status tables are snapshots written while the release
is still a draft. The GitHub Release verification result and linked receiver
workflow runs are the authoritative live status surfaces.
