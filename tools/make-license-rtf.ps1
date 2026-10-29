# Converts LICENSE.md into the RTF that the Windows installer's licence dialog needs.
# WiX's WixUI licence page only accepts RTF, so the markdown file stays the single
# source of truth and this generates the installer copy from it.
param(
    [string]$Source = 'LICENSE.md',
    [string]$Target = 'assets\license.rtf'
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location -Path $Root
$lines = Get-Content -Path $Source -Encoding UTF8

function ConvertTo-RtfText([string]$text) {
    $out = New-Object System.Text.StringBuilder
    foreach ($ch in $text.ToCharArray()) {
        if ($ch -eq '\') { [void]$out.Append('\\') }
        elseif ($ch -eq '{') { [void]$out.Append('\{') }
        elseif ($ch -eq '}') { [void]$out.Append('\}') }
        elseif ([int]$ch -gt 127) { [void]$out.Append('\u' + [int]$ch + '?') }
        else { [void]$out.Append($ch) }
    }
    return $out.ToString()
}

function Clean([string]$text) {
    # Drop markdown emphasis/backtick markers but keep the words; bullets become glyphs.
    $t = $text -replace '\*\*(.+?)\*\*', '$1'
    $t = $t -replace '`([^`]*)`', '$1'
    $t = $t -replace '^>\s*', ''
    $t = $t -replace '^-\s+', (([string][char]0x2022) + ' ')
    return $t
}

$rtf = New-Object System.Text.StringBuilder
[void]$rtf.Append('{\rtf1\ansi\ansicpg1252\deff0\nouicompat\deflang1033')
[void]$rtf.Append('{\fonttbl{\f0\fswiss\fcharset0 Arial;}}')
[void]$rtf.Append('\f0\fs20 ')

foreach ($line in $lines) {
    $text = $line.TrimEnd()
    if ($text -match '^\s*$') { [void]$rtf.Append('\par'); continue }
    if ($text -match '^---+$') { [void]$rtf.Append('\par\pard\brdrb\brdrs\brdrw10\brsp20 \par'); continue }

    $body = ConvertTo-RtfText (Clean $text)
    if ($text -match '^#+\s') {
        $body = ConvertTo-RtfText (Clean ($text -replace '^#+\s+', ''))
    }
    if ($text -match '^# (.+)') {
        [void]$rtf.Append('\pard\sb240\sa120\b\fs32 ' + $body + '\b0\fs20\par')
    } elseif ($text -match '^## (.+)') {
        [void]$rtf.Append('\pard\sb200\sa80\b\fs24 ' + $body + '\b0\fs20\par')
    } elseif ($text -match '^>\s') {
        [void]$rtf.Append('\pard\li280\sa80\i ' + $body + '\i0\par')
    } else {
        [void]$rtf.Append('\pard\sa80 ' + $body + '\par')
    }
}
[void]$rtf.Append('}')

New-Item -ItemType Directory -Force -Path (Split-Path (Join-Path $Root $Target)) | Out-Null
[System.IO.File]::WriteAllText((Join-Path $Root $Target), $rtf.ToString(), [System.Text.Encoding]::ASCII)
Write-Output "wrote $Target ($((Get-Item (Join-Path $Root $Target)).Length) bytes) from $Source"
