# Build and package Areadox Blur for Windows in the OBS 33 plugin layout.
#
#   .\scripts\package.ps1            -> dist\areadox-blur-<version>-windows-x64.zip
#   .\scripts\package.ps1 -Install   -> additionally copy to C:\ProgramData\obs-studio\plugins
#                                      (needs an elevated PowerShell)
#
# Layout (OBS 33+):
#   C:\ProgramData\obs-studio\plugins\areadox-blur\areadox-blur.dll
#   C:\ProgramData\obs-studio\plugins\areadox-blur\data\...
param(
    [switch]$Install
)
$ErrorActionPreference = 'Stop'

$Name = 'areadox-blur'
$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root 'dist'
$Version = (Select-String -Path (Join-Path $Root 'Cargo.toml') -Pattern '^version\s*=\s*"(.+)"' |
    Select-Object -First 1).Matches.Groups[1].Value

Push-Location $Root
try {
    cargo build --release --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'cargo build fehlgeschlagen' }

    if (Test-Path $Dist) { Remove-Item $Dist -Recurse -Force }
    $Pkg = Join-Path $Dist $Name
    New-Item -ItemType Directory -Path $Pkg | Out-Null
    Copy-Item "target\x86_64-pc-windows-msvc\release\areadox_blur.dll" (Join-Path $Pkg "$Name.dll")
    if (Test-Path "target\x86_64-pc-windows-msvc\release\areadox_blur.pdb") {
        Copy-Item "target\x86_64-pc-windows-msvc\release\areadox_blur.pdb" (Join-Path $Pkg "$Name.pdb")
    }
    Copy-Item 'data' (Join-Path $Pkg 'data') -Recurse

    $Zip = Join-Path $Dist "$Name-$Version-windows-x64.zip"
    Compress-Archive -Path $Pkg -DestinationPath $Zip
    Write-Host "Paket: $Zip"

    if ($Install) {
        $Target = Join-Path $env:ProgramData "obs-studio\plugins\$Name"
        if (Test-Path $Target) { Remove-Item $Target -Recurse -Force }
        New-Item -ItemType Directory -Path (Split-Path $Target) -Force | Out-Null
        Copy-Item $Pkg $Target -Recurse
        Write-Host "Installiert nach $Target"
    }
}
finally {
    Pop-Location
}
