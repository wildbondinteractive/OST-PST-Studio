<#
.SYNOPSIS
  Manual Outlook compatibility check for a PST written by OST/PST Studio.

.DESCRIPTION
  This is the one check this project cannot automate honestly: whether classic Outlook
  opens a file without complaint. It is a MANUAL, OPT-IN workflow. It is never run by the
  test suite, and it is not run by the application.

  USE A THROWAWAY OUTLOOK PROFILE. Attaching a store changes the profile it is attached
  to, and a file Outlook cannot load can leave that profile spinning until the store is
  removed again. So do not use your real profile. Create an empty one once:

      & "C:\Program Files\Microsoft Office\root\Office16\OUTLOOK.EXE" /pim OSTPST-Test

  Outlook opens with a profile called OSTPST-Test that has no mail account, only a
  local data file. Leave that window open and run this script. When Outlook asks which
  profile to use next time, pick your normal one; delete OSTPST-Test from Control Panel
  > Mail > Show Profiles when you are done testing.

  What the script does, step by step, with a prompt before anything that touches Outlook:
    1. Copies the PST to a scratch path, so Outlook never touches the original.
    2. Attaches the copy to the running Outlook profile with Namespace.AddStoreEx
       (the same thing File > Open > Outlook Data File does).
    3. Walks every folder and counts items, printing what Outlook sees.
    4. Detaches the store again with Namespace.RemoveStore.

  If Outlook refuses the file, asks for the Inbox Repair Tool (SCANPST), or a folder
  never finishes loading, that is the result to record in docs/OUTLOOK_COMPATIBILITY.md.

.PARAMETER Path
  The PST to check.

.PARAMETER Detach
  Only detach a store with this file name from the running Outlook, for cleaning up
  after a run that did not finish.

.EXAMPLE
  .\scripts\outlook-check.ps1 -Path C:\converted\mailbox.pst
#>
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [switch]$Detach
)

$ErrorActionPreference = 'Stop'

# Outlook must already be running with a window, or every prompt it raises is hidden.
$running = Get-Process OUTLOOK -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle }
if (-not $running) {
    Write-Host "Classic Outlook is not running with a visible window."
    Write-Host "Start it with a throwaway profile, then run this again:"
    Write-Host '  & "C:\Program Files\Microsoft Office\root\Office16\OUTLOOK.EXE" /pim OSTPST-Test'
    Write-Host "New Outlook does not mount PST files and cannot be used for this check."
    exit 3
}
Write-Host "Found running Outlook: $($running[0].MainWindowTitle)"

# Outlook is a single-instance COM server: New-Object connects to the running copy
# rather than starting another, and works in both Windows PowerShell and PowerShell 7.
Write-Host "Connecting to the running Outlook over COM..."
try {
    $outlook = New-Object -ComObject Outlook.Application
} catch {
    Write-Host "Could not attach to the running Outlook: $($_.Exception.Message)"
    Write-Host "If this terminal is elevated (Administrator) and Outlook is not, or the other way"
    Write-Host "round, COM cannot connect them. Run both at the same level."
    exit 3
}
$ns = $outlook.GetNamespace("MAPI")
Write-Host "Connected. Profile: $($ns.CurrentProfileName)."
if ($ns.CurrentProfileName -ne "OSTPST-Test") {
    Write-Host "WARNING: this is not the throwaway profile. A file Outlook cannot load will"
    Write-Host "leave this profile stuck until the store is removed again."
    $go = Read-Host "Continue with profile '$($ns.CurrentProfileName)' anyway? [y/N]"
    if ($go -notmatch '^[Yy]') { Write-Host "Nothing done."; exit 0 }
}

if ($Detach) {
    $name = [IO.Path]::GetFileName($Path)
    $found = $false
    foreach ($s in @($ns.Stores)) {
        if ($s.FilePath -and ([IO.Path]::GetFileName($s.FilePath) -like "*$name*")) {
            Write-Host "Detaching $($s.FilePath)"
            $ns.RemoveStore($s.GetRootFolder())
            $found = $true
        }
    }
    if (-not $found) { Write-Host "No attached store matches '$name'." }
    exit 0
}

if (-not (Test-Path $Path)) { throw "no such file: $Path" }
$Path = (Resolve-Path $Path).Path

# A fresh scratch name every run, so a copy a previous Outlook still holds open cannot
# block this one.
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$scratch = Join-Path $env:TEMP ("ostpst-outlook-check-" + $stamp + "-" + [IO.Path]::GetFileName($Path))
Copy-Item -Force $Path $scratch
Write-Host "Copied to $scratch (Outlook will open the copy, never the original)."

$answer = Read-Host "Attach this copy to the running Outlook profile and read it back? [y/N]"
if ($answer -notmatch '^[Yy]') { Write-Host "Nothing done."; exit 0 }

Write-Host "Attaching the store (watch the Outlook window for any prompt)..."
$sw = [Diagnostics.Stopwatch]::StartNew()
try {
    # olStoreUnicode = 3
    $ns.AddStoreEx($scratch, 3)
} catch {
    Write-Host "Outlook REFUSED the file after $([int]$sw.Elapsed.TotalSeconds) s: $($_.Exception.Message)"
    Write-Host "Result: NOT compatible as written. Note whether it asked for SCANPST."
    exit 2
}
Write-Host "Attached in $([int]$sw.Elapsed.TotalSeconds) s."
$store = $ns.Stores | Where-Object { $_.FilePath -eq $scratch } | Select-Object -First 1
if (-not $store) { throw "the store was added but could not be found again" }

function Walk($folder, $depth) {
    $count = 0
    try { $count = $folder.Items.Count } catch { $count = -1 }
    Write-Host ("{0}{1}  ({2} items)" -f ('  ' * $depth), $folder.Name, $count)
    $total = [Math]::Max(0, $count)
    foreach ($sub in $folder.Folders) { $total += Walk $sub ($depth + 1) }
    return $total
}
Write-Host "`nOutlook sees:"
$total = Walk $store.GetRootFolder() 0
Write-Host "`nTotal items visible to Outlook: $total"
Write-Host "Look at the Outlook window now: the store appears in the folder pane as '$($store.DisplayName)'."
Write-Host "Click into its folders and open a message. Note anything that spins, errors or asks for SCANPST."

Read-Host "Press Enter to detach the store from the profile" | Out-Null
$ns.RemoveStore($store.GetRootFolder())
Write-Host "Detached. The profile is as it was. Scratch copy left at $scratch."
Write-Host "Result: Outlook attached the file and listed $total item(s) through COM."
exit 0
