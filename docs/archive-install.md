# Verify and install a GitHub Release archive

Use this guide when installing a native Git Slop archive directly instead of
using Homebrew, Cargo Binstall, or Scoop. The example verifies the immutable
release record, schema-3 manifest identity, archive digest and size,
attestation, and installed binary identity before treating the install as
trusted.

Windows users should follow the
[PowerShell archive guide](archive-install-windows.md).

## Unix

Set `target` to the supported archive target for the host, then run:

```bash
release=v0.15.0
target=x86_64-unknown-linux-gnu
version="${release#v}"
archive="git-slop-${release}-${target}.tar.gz"
root="git-slop-${release}-${target}"
gh release download "$release" \
  --repo coreycoto/git-slop \
  --pattern "$archive" \
  --pattern SHA256SUMS \
  --pattern release-manifest.json
gh release verify "$release" --repo coreycoto/git-slop

IFS=$'\t' read -r revision crate_sha256 < <(
  jq -er --arg version "$version" --arg release "$release" '
    select(
      .schema_version == 3
      and .project == "git-slop"
      and .repository == "coreycoto/git-slop"
      and .version == $version
      and .tag == $release
      and (.revision | test("^[0-9a-f]{40}$"))
      and .crate_source.version == $version
      and .crate_source.revision == .revision
      and .crate_source.vcs_dirty == false
      and (.crate_source.sha256 | test("^[0-9a-f]{64}$"))
      and (.artifacts | length == 7)
    )
    | [.revision, .crate_source.sha256]
    | @tsv
  ' release-manifest.json
)
IFS=$'\t' read -r manifest_sha256 manifest_size < <(
  jq -er --arg target "$target" --arg archive "$archive" '
    [.artifacts[] | select(.target == $target)]
    | select(length == 1)
    | .[0]
    | select(
        .name == $archive
        and .path == $archive
        and (.url | endswith("/" + $archive))
        and (.sha256 | test("^[0-9a-f]{64}$"))
        and (.size_bytes | type == "number" and . > 0)
      )
    | [.sha256, .size_bytes]
    | @tsv
  ' release-manifest.json
)
checksum_sha256="$(
  awk -v name="$archive" '
    $2 == name { count += 1; digest = $1 }
    END { if (count != 1) exit 1; print digest }
  ' SHA256SUMS
)"
if command -v sha256sum >/dev/null 2>&1; then
  actual_sha256="$(sha256sum "$archive" | awk '{print $1}')"
else
  actual_sha256="$(shasum -a 256 "$archive" | awk '{print $1}')"
fi
actual_size="$(wc -c <"$archive" | tr -d '[:space:]')"
test "$actual_sha256" = "$manifest_sha256"
test "$actual_sha256" = "$checksum_sha256"
test "$actual_size" = "$manifest_size"
gh attestation verify "$archive" \
  --repo coreycoto/git-slop \
  --signer-repo coreycoto/git-slop

tar --no-same-owner -xzf "$archive"
mkdir -p \
  "$HOME/.local/bin" \
  "$HOME/.local/share/man/man1" \
  "$HOME/.local/share/bash-completion/completions" \
  "$HOME/.zfunc" \
  "$HOME/.config/fish/completions"
install -m 0755 "$root/git-slop" "$HOME/.local/bin/git-slop"
install -m 0644 "$root/man/git-slop.1" "$HOME/.local/share/man/man1/git-slop.1"
install -m 0644 "$root/completions/git-slop.bash" \
  "$HOME/.local/share/bash-completion/completions/git-slop"
install -m 0644 "$root/completions/git-slop.zsh" "$HOME/.zfunc/_git-slop"
install -m 0644 "$root/completions/git-slop.fish" \
  "$HOME/.config/fish/completions/git-slop.fish"

build_info="$("$HOME/.local/bin/git-slop" build-info --format json)"
jq -e \
  --arg version "$version" \
  --arg revision "$revision" \
  --arg target "$target" \
  --arg crate_sha256 "$crate_sha256" '
    .schema_version == 2
    and .project == "git-slop"
    and .version == $version
    and .source_revision == $revision
    and .source_dirty == false
    and .target == $target
    and .crate_sha256 == $crate_sha256
    and .build_source == "release"
  ' <<<"$build_info"
```

The recipe creates every destination and installs the bundled Bash, Zsh, and
Fish completion sources. Zsh users should add the following to `.zshrc`:

```bash
fpath=("$HOME/.zfunc" $fpath)
autoload -Uz compinit && compinit
```

For a direct archive update, verify the new archive before replacing the same
executable and manual targets. To uninstall, remove `$HOME/.local/bin/git-slop`,
`$HOME/.local/share/man/man1/git-slop.1`, and the completion files installed by
the recipe.
