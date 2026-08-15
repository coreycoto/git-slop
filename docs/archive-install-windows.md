# Verify and install a GitHub Release archive on Windows

Use this PowerShell guide for a direct Windows archive installation. Use
`aarch64-pc-windows-msvc` instead on Windows ARM64.

```powershell
$Release = "v0.14.0"
$Target = "x86_64-pc-windows-msvc"
$Version = $Release.TrimStart("v")
$Archive = "git-slop-$Release-$Target.zip"
$Root = "git-slop-$Release-$Target"
gh release download $Release --repo coreycoto/git-slop --pattern $Archive --pattern SHA256SUMS --pattern release-manifest.json
gh release verify $Release --repo coreycoto/git-slop
$Manifest = Get-Content -Raw release-manifest.json | ConvertFrom-Json
if (
  $Manifest.schema_version -ne 3 -or
  $Manifest.project -ne "git-slop" -or
  $Manifest.repository -ne "coreycoto/git-slop" -or
  $Manifest.version -ne $Version -or
  $Manifest.tag -ne $Release -or
  $Manifest.revision -notmatch "^[0-9a-f]{40}$" -or
  $Manifest.crate_source.version -ne $Version -or
  $Manifest.crate_source.revision -ne $Manifest.revision -or
  $Manifest.crate_source.vcs_dirty -ne $false -or
  $Manifest.crate_source.sha256 -notmatch "^[0-9a-f]{64}$" -or
  @($Manifest.artifacts).Count -ne 7
) { throw "release-manifest.json identity mismatch" }
$Artifacts = @($Manifest.artifacts | Where-Object target -EQ $Target)
if (
  $Artifacts.Count -ne 1 -or
  $Artifacts[0].name -ne $Archive -or
  $Artifacts[0].path -ne $Archive -or
  -not $Artifacts[0].url.EndsWith("/$Archive") -or
  $Artifacts[0].sha256 -notmatch "^[0-9a-f]{64}$" -or
  $Artifacts[0].size_bytes -le 0
) { throw "release manifest target mismatch" }
$ChecksumLines = @(Get-Content SHA256SUMS | Where-Object { $_ -match "^[0-9a-f]{64}  $([regex]::Escape($Archive))$" })
if ($ChecksumLines.Count -ne 1) { throw "SHA256SUMS must contain the archive exactly once" }
$Expected = $ChecksumLines[0].Split()[0]
$Actual = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
if ($Actual -ne $Expected -or $Actual -ne $Artifacts[0].sha256) { throw "SHA-256 mismatch for $Archive" }
if ((Get-Item $Archive).Length -ne $Artifacts[0].size_bytes) { throw "archive size mismatch" }
gh attestation verify $Archive --repo coreycoto/git-slop --signer-repo coreycoto/git-slop
Expand-Archive -LiteralPath $Archive -DestinationPath . -Force
$Install = Join-Path $env:LOCALAPPDATA "Programs\git-slop\bin"
New-Item -ItemType Directory -Force -Path $Install | Out-Null
Copy-Item "$Root\git-slop.exe" $Install
Copy-Item "$Root\completions" $Install -Recurse -Force
[Environment]::SetEnvironmentVariable("Path", "$Install;$([Environment]::GetEnvironmentVariable('Path','User'))", "User")
$Completion = Join-Path $Install "completions\git-slop.powershell"
$ProfileDirectory = Split-Path -Parent $PROFILE
New-Item -ItemType Directory -Force -Path $ProfileDirectory | Out-Null
if (-not (Test-Path -LiteralPath $PROFILE)) {
  New-Item -ItemType File -Path $PROFILE | Out-Null
}
$Activation = ". '$Completion'"
if (-not (Select-String -LiteralPath $PROFILE -SimpleMatch $Activation -Quiet)) { Add-Content -LiteralPath $PROFILE $Activation }
. $Completion
$Info = & "$Install\git-slop.exe" build-info --format json | ConvertFrom-Json
if (
  $Info.schema_version -ne 2 -or
  $Info.project -ne "git-slop" -or
  $Info.version -ne $Version -or
  $Info.source_revision -ne $Manifest.revision -or
  $Info.source_dirty -ne $false -or
  $Info.target -ne $Target -or
  $Info.crate_sha256 -ne $Manifest.crate_source.sha256 -or
  $Info.build_source -ne "release"
) { throw "installed build identity mismatch" }
```

Start a new shell after updating the user `PATH`. To update, verify the new
archive before replacing the executable. To uninstall, remove
`$env:LOCALAPPDATA\Programs\git-slop`, then remove only that exact directory
from the user `Path` value.
