# Builds a multi-size .ico from a master PNG. Each entry is a PNG-encoded image,
# which Windows Vista+ icon handling supports (small sizes stay sharp under DPI scaling).
param(
    [string]$Source = 'D:\Github\api-checker\assets\icon-master.png',
    [string]$Target = 'D:\Github\api-checker\assets\techvision-api-checker.ico'
)

Add-Type -AssemblyName System.Drawing

$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
$master = [System.Drawing.Image]::FromFile($Source)

$blobs = @()
foreach ($size in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $rect = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
    $g.DrawImage($master, $rect)
    $g.Dispose()

    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    $blobs += ,($ms.ToArray())
    $ms.Dispose()
}
$master.Dispose()

$iconDirBytes = 6
$entryBytes = 16 * $sizes.Count
$offset = $iconDirBytes + $entryBytes

$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($out)
$w.Write([UInt16]0)          # reserved
$w.Write([UInt16]1)          # type: icon
$w.Write([UInt16]$sizes.Count)

for ($i = 0; $i -lt $sizes.Count; $i++) {
    $size = $sizes[$i]
    $blob = $blobs[$i]
    $w.Write([Byte]$(if ($size -ge 256) { 0 } else { $size }))
    $w.Write([Byte]$(if ($size -ge 256) { 0 } else { $size }))
    $w.Write([Byte]0)        # palette count
    $w.Write([Byte]0)        # reserved
    $w.Write([UInt16]1)      # planes
    $w.Write([UInt16]32)     # bit depth
    $w.Write([UInt32]$blob.Length)
    $w.Write([UInt32]$offset)
    $offset += $blob.Length
}

foreach ($blob in $blobs) { $w.Write($blob) }

New-Item -ItemType Directory -Force -Path (Split-Path $Target) | Out-Null
[System.IO.File]::WriteAllBytes($Target, $out.ToArray())
$w.Dispose(); $out.Dispose()

Write-Output "wrote $Target ($((Get-Item $Target).Length) bytes, $($sizes.Count) sizes)"

# Read it back through the shell's icon loader to prove it parses.
$probe = [System.Drawing.Icon]::ExtractAssociatedIcon($Target)
Write-Output "parsed as icon: $($probe.Width)x$($probe.Height)"
$probe.Dispose()
