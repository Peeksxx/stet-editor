param(
    [string]$Compiler = 'ISCC.exe',
    [string]$OutputDirectory = '',
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
if ($OutputDirectory -eq '') { $OutputDirectory = Join-Path $project 'release' }
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
Push-Location $project
try {
    if (-not $SkipBuild) {
        & cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    }
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $project 'target' }
    $binary = Join-Path $target 'release\stet.exe'
    if (-not (Test-Path -LiteralPath $binary)) { throw "Binary missing: $binary" }
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    & $Compiler "/DBinaryPath=$binary" "/DOutputPath=$OutputDirectory" (Join-Path $project 'installer\stet.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
    $installer = Join-Path $OutputDirectory 'Stet-Setup-x64.exe'
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  Stet-Setup-x64.exe" | Set-Content -LiteralPath (Join-Path $OutputDirectory 'SHA256SUMS.txt') -Encoding ascii
    Write-Output $installer
} finally { Pop-Location }
