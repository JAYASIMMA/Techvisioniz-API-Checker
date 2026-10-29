# Reads MSI tables through the Windows Installer COM API (no install performed).
param([string]$Msi = 'D:\Github\api-checker\dist\Techvision-Api-Checker-1.0.0.msi')

$installer = New-Object -ComObject WindowsInstaller.Installer
$db = $installer.GetType().InvokeMember('OpenDatabase', 'InvokeMethod', $null, $installer, @($Msi, 0))

function Get-Column($sql, $col) {
    $view = $db.GetType().InvokeMember('OpenView', 'InvokeMethod', $null, $db, $sql)
    [void]$view.GetType().InvokeMember('Execute', 'InvokeMethod', $null, $view, $null)
    $rows = @()
    while ($true) {
        $rec = $view.GetType().InvokeMember('Fetch', 'InvokeMethod', $null, $view, $null)
        if ($null -eq $rec) { break }
        $rows += $rec.GetType().InvokeMember('StringData', 'GetProperty', $null, $rec, @($col))
    }
    [void]$view.GetType().InvokeMember('Close', 'InvokeMethod', $null, $view, $null)
    , $rows
}

"--- Product properties ---"
$props = @('ProductName', 'ProductVersion', 'ProductCode', 'UpgradeCode', 'Manufacturer', 'InstallScope', 'ARPPRODUCTICON', 'ARPCONTACT', 'ALLUSERS', 'MSIINSTALLPERUSER')
foreach ($name in $props) {
    $vals = Get-Column "SELECT Value FROM Property WHERE Property = '$name'" 1
    if ($vals.Count) { "{0,-18} = {1}" -f $name, $vals[0] }
}

"`n--- Files shipped ---"
Get-Column "SELECT FileName FROM File" 1 | ForEach-Object { $_ }

"`n--- Directories ---"
Get-Column "SELECT Directory, DefaultDir FROM Directory" 1 | ForEach-Object { $_ }
Get-Column "SELECT DefaultDir FROM Directory WHERE Directory = 'INSTALLFOLDER'" 1 | ForEach-Object { "  install target: $_" }

"`n--- Package architecture (summary stream) ---"
# _Information is a hidden table; the architecture lives in the metadata streams.
$db.GetType().InvokeMember('ISummaryInformation', 'SetProperty', $null, $db, $true) | Out-Null
$si = $db.GetType().InvokeMember('SummaryInformation', 'GetProperty', $null, $db, $null)
function Get-SiProp($si, $n) {
    try { return $si.GetType().InvokeMember('Property', 'InvokeMethod', $null, $si, @($n)) }
    catch { return $si.GetType().InvokeMember('Property', 'GetProperty', $null, $si, @($n)) }
}
$template = Get-SiProp $si 7     # PIDSI_TEMPLATE: "<arch>;<language>"
$revision = Get-SiProp $si 11    # PIDSI_REVNUMBER: package type / language
$bits = ($template -split ';')[0]
$archNames = @{ 'x64' = '64-bit (AMD64)'; 'Intel' = '32-bit (x86)'; 'arm64' = 'ARM64' }
$label = if ($archNames.ContainsKey($bits)) { $archNames[$bits] } else { "unrecognised ('$bits')" }
"  template: $template  ->  $label"
"  revision: $revision"

"`n--- Media (cabinet #num) ---"
Get-Column "SELECT Cabinet FROM Media" 1 | ForEach-Object { if ($_ -eq '') { '(none)' } else { "#$_" } }

"`n--- Shortcuts ---"
Get-Column "SELECT Name FROM Shortcut" 1 | ForEach-Object { $_ }
Get-Column "SELECT Target FROM Shortcut" 1 | ForEach-Object { "  -> $_" }

"`n--- Features ---"
Get-Column "SELECT Feature FROM Feature" 1 | ForEach-Object { $_ }

"`n--- Wizard UI resources (Binary table) ---"
Get-Column "SELECT Name FROM Binary" 1 | ForEach-Object { "  $_" }

"`n--- Licence page (Dialog table) ---"
$dialogs = Get-Column "SELECT Dialog FROM Dialog" 1
"  dialogs: $($dialogs.Count)"
foreach ($d in $dialogs) { if ($d -match '(?i)licen') { "  found: $d" } }
