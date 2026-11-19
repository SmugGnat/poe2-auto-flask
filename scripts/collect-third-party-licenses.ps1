param(
    [Parameter(Mandatory = $true)]
    [string]$OutputDir,

    [Parameter(Mandatory = $true)]
    [string]$NoticePath,

    [string]$ManifestPath = "app-ui/Cargo.toml"
)

$ErrorActionPreference = "Stop"

$metadataJson = cargo metadata --locked --format-version 1 --manifest-path $ManifestPath --filter-platform x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata failed with exit code $LASTEXITCODE"
}

$metadata = $metadataJson | ConvertFrom-Json
$packages = @(
    $metadata.packages |
        Where-Object { $_.source -like "registry+*" } |
        Sort-Object name, version
)

if (Test-Path -LiteralPath $OutputDir) {
    Remove-Item -LiteralPath $OutputDir -Recurse -Force
}
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

$summary = [System.Collections.Generic.List[string]]::new()
$summary.Add("# Third-party dependency licenses")
$summary.Add("")
$summary.Add("Generated from the exact registry packages selected by Cargo.lock.")
$summary.Add("")
$summary.Add("| Package | Version | Declared license |")
$summary.Add("| --- | --- | --- |")

$notice = [System.Collections.Generic.List[string]]::new()
$notice.Add("PoE2 Auto Flask - Third-Party Dependency Notices")
$notice.Add("")
$notice.Add("Generated from the exact registry packages selected by Cargo.lock.")
$notice.Add("")

foreach ($package in $packages) {
    $crateDir = Split-Path -Parent $package.manifest_path
    $licenseFiles = [System.Collections.Generic.List[System.IO.FileInfo]]::new()

    if ($package.license_file) {
        $declaredLicenseFile = Join-Path $crateDir $package.license_file
        if (Test-Path -LiteralPath $declaredLicenseFile -PathType Leaf) {
            $licenseFiles.Add((Get-Item -LiteralPath $declaredLicenseFile))
        }
    }

    Get-ChildItem -LiteralPath $crateDir -File |
        Where-Object { $_.Name -match '^(LICENSE|COPYING|UNLICENSE|NOTICE)([-._].*)?$' } |
        ForEach-Object { $licenseFiles.Add($_) }

    Get-ChildItem -LiteralPath $crateDir -Directory |
        Where-Object { $_.Name -match '^LICENSES?$' } |
        ForEach-Object {
            Get-ChildItem -LiteralPath $_.FullName -File -Recurse |
                ForEach-Object { $licenseFiles.Add($_) }
        }

    $licenseFiles = @($licenseFiles | Sort-Object FullName -Unique)
    if ($licenseFiles.Count -eq 0) {
        # A few crates omit their upstream license texts from the crate archive.
        # Version-specific copies and their commit URLs are kept in the repo.
        $fallback = Join-Path $PSScriptRoot "../packaging/licenses/$($package.name)-$($package.version)"
        if (Test-Path -LiteralPath $fallback -PathType Container) {
            $licenseFiles = @(Get-ChildItem -LiteralPath $fallback -File)
        }
    }
    if ($licenseFiles.Count -eq 0) {
        throw "No packaged license/notice file found for $($package.name) $($package.version)"
    }

    $packageDir = Join-Path $OutputDir "$($package.name)-$($package.version)"
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null

    $license = if ([string]::IsNullOrWhiteSpace($package.license)) {
        "See packaged license file"
    } else {
        $package.license.Replace("|", "\|")
    }
    $summary.Add("| $($package.name) | $($package.version) | ``$license`` |")

    $notice.Add("=" * 78)
    $notice.Add("$($package.name) $($package.version)")
    $notice.Add("Declared license: $($package.license)")
    $notice.Add("=" * 78)
    $notice.Add("")

    foreach ($licenseFile in $licenseFiles) {
        $destination = Join-Path $packageDir $licenseFile.Name
        Copy-Item -LiteralPath $licenseFile.FullName -Destination $destination

        $notice.Add("--- $($licenseFile.Name) ---")
        $notice.Add("")
        $notice.Add((Get-Content -LiteralPath $licenseFile.FullName -Raw).TrimEnd())
        $notice.Add("")
    }
}

$summary.Add("")
$summary.Add("License files come from the Cargo registry or the version-matched upstream supplements in packaging/licenses. The upstream license terms govern those dependencies.")
$summary | Set-Content -LiteralPath (Join-Path $OutputDir "README.md") -Encoding utf8

$noticeParent = Split-Path -Parent $NoticePath
if ($noticeParent) {
    New-Item -ItemType Directory -Path $noticeParent -Force | Out-Null
}
$notice.Add("=" * 78)
$notice.Add("Inter Variable font")
$notice.Add("=" * 78)
$notice.Add((Get-Content -LiteralPath "app-ui/ui/fonts/OFL.txt" -Raw).TrimEnd())
$notice | Set-Content -LiteralPath $NoticePath -Encoding utf8
