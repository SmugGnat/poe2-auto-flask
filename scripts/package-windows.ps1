param(
    [string]$ExecutablePath = "app-ui/target/x86_64-pc-windows-msvc/release/poe2-auto-flask-ui.exe",
    [string]$OutputDir = "dist"
)

$ErrorActionPreference = "Stop"
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("poe-auto-flask-licenses-" + [guid]::NewGuid())

try {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
    Copy-Item -LiteralPath $ExecutablePath -Destination (Join-Path $OutputDir "poe2-auto-flask-windows-x64.exe")

    $thirdParty = Join-Path $temporary "THIRD_PARTY_NOTICES.txt"
    & "$PSScriptRoot/collect-third-party-licenses.ps1" `
        -OutputDir (Join-Path $temporary "dependencies") `
        -NoticePath $thirdParty

    @(
        'PoE Auto Flask'
        'GPL-3.0-only and third-party dependency licenses'
        ''
        '=============================================================================='
        'PROJECT LICENSE'
        '=============================================================================='
        ''
        (Get-Content -LiteralPath LICENSE -Raw).TrimEnd()
        ''
        '=============================================================================='
        'THIRD-PARTY DEPENDENCY LICENSES'
        '=============================================================================='
        ''
        (Get-Content -LiteralPath $thirdParty -Raw).TrimEnd()
    ) | Set-Content -LiteralPath (Join-Path $OutputDir "LICENSES.txt") -Encoding utf8
}
finally {
    if (Test-Path -LiteralPath $temporary) {
        Remove-Item -LiteralPath $temporary -Recurse -Force
    }
}
