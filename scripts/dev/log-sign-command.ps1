<#
.SYNOPSIS
  Stand-in for the sign command, for checking what the Tauri bundler hands it.

.DESCRIPTION
  Signs nothing. Appends one line per invocation to scripts/dev/sign-command.log: the
  working directory the bundler used, the file, its size, and for ostpst-studio.exe the
  bundle-type marker the bundler patched into it (UNK, NSS or MSI). Used by
  staged-packaging-check.ps1 to prove that the bundler calls the sign command for each
  patched copy of the executable and for each installer, which is what the real
  scripts/azure-sign.ps1 relies on.
#>
param([Parameter(Mandatory = $true)][string]$File)
$log = Join-Path $PSScriptRoot "sign-command.log"
$marker = ""
if ((Split-Path $File -Leaf) -ieq "ostpst-studio.exe") {
    $bytes = [System.IO.File]::ReadAllBytes($File)
    $needle = [System.Text.Encoding]::ASCII.GetBytes("BUNDLE_TYPE_VAR_")
    for ($i = 0; $i -le $bytes.Length - $needle.Length - 3; $i++) {
        $hit = $true
        for ($j = 0; $j -lt $needle.Length; $j++) { if ($bytes[$i + $j] -ne $needle[$j]) { $hit = $false; break } }
        if ($hit) { $marker = [System.Text.Encoding]::ASCII.GetString($bytes, $i + $needle.Length, 3); break }
    }
}
$size = (Get-Item $File).Length
"cwd=$PWD file=$File size=$size marker=$marker" | Add-Content $log
Write-Host "sign-command stand-in: $File ($size bytes) marker=$marker"
