# Generates the two BMPs the Windows installer wizard uses.
#
# WiX paints its own labels on top of these images: the page title goes on the banner
# at the top-left, and the body text fills the dialog's right side. Any text baked into
# the bitmaps collides with it, so this art is deliberately text-free outside the
# dialog's left brand band, and the banner stays light for readable black titles.
param(
    [string]$OutDir = 'assets'
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location -Path $Root
Add-Type -AssemblyName System.Drawing

$teal = [System.Drawing.Color]::FromArgb(255, 15, 118, 110)
$cyan = [System.Drawing.Color]::FromArgb(255, 45, 212, 191)
$white = [System.Drawing.Color]::White
$ink = [System.Drawing.Color]::FromArgb(255, 23, 32, 42)

function Save-Bmp($bmp, $name) {
    $path = Join-Path $Root (Join-Path $OutDir $name)
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Bmp)
    $bmp.Dispose()
    Write-Output "  $path ($((Get-Item $path).Length) bytes)"
}

function New-Gradient($rect) {
    New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, $teal, $cyan,
        [System.Drawing.Drawing2D.LinearGradientMode]::ForwardDiagonal)
}

function Set-Quality($g) {
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::ClearTypeGridFit
}

# Draws the app's mark (magnifier + check + angle brackets) with primitives only.
# Graphics.DrawImage(Image, ...) cannot be bound reliably from PowerShell, and the
# vector shapes also stay crisp at this size.
function Draw-BrandMark($g, $cx, $cy, $scale, $pen) {
    $r = 34 * $scale
    $g.DrawEllipse($pen, ($cx - $r), ($cy - $r), (2 * $r), (2 * $r))
    # handle, running out from the lower-right of the lens
    $g.DrawLine($pen, ($cx + $r * 0.72), ($cy + $r * 0.72), ($cx + $r * 1.55), ($cy + $r * 1.55))
    # check inside the lens
    $check = @(
        (New-Object System.Drawing.PointF(($cx - $r * 0.45), ($cy + $r * 0.02))),
        (New-Object System.Drawing.PointF(($cx - $r * 0.10), ($cy + $r * 0.40))),
        (New-Object System.Drawing.PointF(($cx + $r * 0.50), ($cy - $r * 0.32)))
    )
    $g.DrawLines($pen, [System.Drawing.PointF[]]$check)
    # angle brackets either side
    $gap = $r * 1.85
    $left = @(
        (New-Object System.Drawing.PointF(($cx - $gap), ($cy - $r * 0.45))),
        (New-Object System.Drawing.PointF(($cx - $gap - $r * 0.42), $cy)),
        (New-Object System.Drawing.PointF(($cx - $gap), ($cy + $r * 0.45)))
    )
    $right = @(
        (New-Object System.Drawing.PointF(($cx + $gap), ($cy - $r * 0.45))),
        (New-Object System.Drawing.PointF(($cx + $gap + $r * 0.42), $cy)),
        (New-Object System.Drawing.PointF(($cx + $gap), ($cy + $r * 0.45)))
    )
    $g.DrawLines($pen, [System.Drawing.PointF[]]$left)
    $g.DrawLines($pen, [System.Drawing.PointF[]]$right)
}

# --- banner: 493x58. The wizard draws its page title at the top-left, so keep the
# left side plain white and put the mark at the far right where nothing is written. ---
$banner = New-Object System.Drawing.Bitmap(493, 58, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$g = [System.Drawing.Graphics]::FromImage($banner)
Set-Quality $g
$g.Clear($white)
$rule = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Rectangle(0, 55, 493, 3)), $teal, $cyan,
    [System.Drawing.Drawing2D.LinearGradientMode]::Horizontal)
$g.FillRectangle($rule, 0, 55, 493, 3)
$pen = New-Object System.Drawing.Pen($teal, 4.5)
$pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
Draw-BrandMark $g 462 27 0.34 $pen
$pen.Dispose()
$g.Dispose()
Save-Bmp $banner 'installer-banner.bmp'

# --- dialog: 493x312. Brand band on the left, plain white where WiX writes text. ---
$dialog = New-Object System.Drawing.Bitmap(493, 312, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$g = [System.Drawing.Graphics]::FromImage($dialog)
Set-Quality $g
$g.Clear($white)
$band = New-Object System.Drawing.Rectangle(0, 0, 180, 312)
$g.FillRectangle((New-Gradient $band), $band)

$pen = New-Object System.Drawing.Pen($white, 11.0)
$pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
Draw-BrandMark $g 90 132 1.0 $pen
$pen.Dispose()

# Only the band carries text; it is never overlapped by wizard labels.
$brandFont = New-Object System.Drawing.Font('Segoe UI', 13, [System.Drawing.FontStyle]::Bold)
$subFont = New-Object System.Drawing.Font('Segoe UI', 8.5, [System.Drawing.FontStyle]::Regular)
$fmt = New-Object System.Drawing.StringFormat
$fmt.Alignment = [System.Drawing.StringAlignment]::Center
$brandRect = New-Object System.Drawing.RectangleF(6, 214, 168, 26)
$g.DrawString('Techvision', $brandFont, (New-Object System.Drawing.SolidBrush $white), $brandRect, $fmt)
$g.DrawString('API Checker', $brandFont, (New-Object System.Drawing.SolidBrush $white),
    (New-Object System.Drawing.RectangleF(6, 238, 168, 26)), $fmt)
# Two short lines: a single longer string overflows the 168px band and gets clipped.
$g.DrawString('HTTP client', $subFont, (New-Object System.Drawing.SolidBrush $white),
    (New-Object System.Drawing.RectangleF(6, 268, 168, 18)), $fmt)
$g.DrawString('for Windows and Linux', $subFont, (New-Object System.Drawing.SolidBrush $white),
    (New-Object System.Drawing.RectangleF(6, 285, 168, 18)), $fmt)
$brandFont.Dispose(); $subFont.Dispose()
$g.Dispose()
Save-Bmp $dialog 'installer-dialog.bmp'

Write-Output 'installer art regenerated (text-free outside the brand band)'
