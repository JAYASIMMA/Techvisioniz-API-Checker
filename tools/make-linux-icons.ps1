# Writes the hicolor icon theme PNGs that the .deb, .rpm and AppImage packages install.
param(
    [string]$Source = 'assets\icon-master.png',
    [string]$OutDir = 'packaging\linux\icons'
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location -Path $Root
Add-Type -AssemblyName System.Drawing

$master = [System.Drawing.Image]::FromFile((Join-Path $Root $Source))
foreach ($size in @(16, 24, 32, 48, 64, 128, 256)) {
    $dir = Join-Path $Root "$OutDir\${size}x${size}"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.DrawImage($master, (New-Object System.Drawing.Rectangle(0, 0, $size, $size)))
    $g.Dispose()
    $bmp.Save((Join-Path $dir 'techvision-api-checker.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Output "  ${size}x${size}"
}
$master.Dispose()
Write-Output "hicolor icons written under $OutDir"
