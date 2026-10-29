# Builds Techvision API Checker for 64-bit and 32-bit Windows and packages each
# into its own MSI.
#
#   powershell -ExecutionPolicy Bypass -File build-installer.ps1              # both
#   powershell -ExecutionPolicy Bypass -File build-installer.ps1 -Arch x64     # 64-bit only
#   powershell -ExecutionPolicy Bypass -File build-installer.ps1 -SkipExeBuild # repackage only
param(
    [ValidateSet('all', 'x64', 'x86')]
    [string]$Arch = 'all',
    [switch]$SkipExeBuild
)

$ErrorActionPreference = 'Stop'
Set-Location -Path $PSScriptRoot

$version = (Select-String -Path Cargo.toml -Pattern '^\s*version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value

# Stable, per-architecture upgrade codes: installing one architecture never
# removes the other, but a new release of the same architecture upgrades in place.
$targets = [ordered]@{
    x64 = [pscustomobject]@{
        Triple         = 'x86_64-pc-windows-msvc'
        Machine        = 0x8664
        UpgradeCode    = '2634052a-3a4d-53a6-9cd2-30689e45d1f5'
        InstallDirName = 'Techvision API Checker (64-bit)'
        WixArch        = 'x64'
    }
    x86 = [pscustomobject]@{
        Triple         = 'i686-pc-windows-msvc'
        Machine        = 0x014C
        UpgradeCode    = '642f0763-89d7-5de6-b375-bfdf28e9457b'
        InstallDirName = 'Techvision API Checker (32-bit)'
        WixArch        = 'x86'
    }
}

$wix = Join-Path $env:USERPROFILE '.dotnet\tools\wix.exe'
if (-not (Test-Path $wix)) {
    throw 'WiX v5 missing. Run: dotnet tool install --global wix --version 5.0.2'
}
$extensions = (& $wix extension list -g) -join ' '
if ($extensions -notmatch 'WixToolset\.UI\.wixext') {
    throw 'UI extension missing. Run: wix extension add -g WixToolset.UI.wixext/5.0.2'
}

function Get-PeMachine([string]$Path) {
    # PE/COFF header: e_lfanew at 0x3C, then 'PE\0\0' and the Machine word.
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
    if ([BitConverter]::ToUInt32($bytes, $peOffset) -ne 0x00004550) { throw "$Path is not a PE file" }
    return [BitConverter]::ToUInt16($bytes, $peOffset + 4)
}

New-Item -ItemType Directory -Force -Path dist | Out-Null

# Regenerate every derived asset from its source so the installer can't ship a stale
# licence text or icon.
Write-Output '--- regenerating derived assets ---'
& (Join-Path $PSScriptRoot 'assets\make-icon.ps1') | Out-Null
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'tools\make-linux-icons.ps1') | Out-Null
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'tools\make-license-rtf.ps1') | Out-Null
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'tools\make-installer-bitmaps.ps1') | Out-Null
foreach ($required in @('assets\techvision-api-checker.ico', 'assets\license.rtf', 'assets\installer-banner.bmp', 'assets\installer-dialog.bmp')) {
    if (-not (Test-Path (Join-Path $PSScriptRoot $required))) { throw "missing generated asset: $required" }
}
Write-Output 'assets ok'

$built = @()

foreach ($name in $targets.Keys) {
    if ($Arch -ne 'all' -and $Arch -ne $name) { continue }
    $t = $targets[$name]

    Write-Output "`n=== $name ($($t.Triple)) ==="
    $exe = Join-Path $PSScriptRoot "target\$($t.Triple)\release\techvision-api-checker.exe"

    if (-not $SkipExeBuild) {
        cargo build --release --target $t.Triple
        if ($LASTEXITCODE) { throw "cargo build --target $($t.Triple) failed" }
    }
    if (-not (Test-Path $exe)) { throw "expected binary not found: $exe" }

    $machine = Get-PeMachine $exe
    if ($machine -ne $t.Machine) {
        throw "$name binary has PE machine 0x$($machine.ToString('X4')), expected 0x$($t.Machine.ToString('X4'))"
    }
    Write-Output ("binary ok: 0x{0:X4} ({1:N2} MB)" -f $machine, ((Get-Item $exe).Length / 1MB))

    $msi = Join-Path $PSScriptRoot "dist\Techvision-Api-Checker-$version-$name.msi"
    & $wix build wix\main.wxs `
        -arch $t.WixArch `
        -ext WixToolset.UI.wixext `
        -d "Version=$version" `
        -d "UpgradeCode=$($t.UpgradeCode)" `
        -d "ExePath=$exe" `
        -d "InstallDirName=$($t.InstallDirName)" `
        -d "ArchKey=$name" `
        -o $msi
    if ($LASTEXITCODE) { throw "wix build failed for $name" }

    $item = Get-Item $msi
    $built += [pscustomobject]@{ Arch = $name; Msi = $item.FullName; MB = [math]::Round($item.Length / 1MB, 2) }
}

Write-Output "`n=== installers ==="
$built | ForEach-Object { "{0,-4} {1} ({2} MB)" -f $_.Arch, $_.Msi, $_.MB }
