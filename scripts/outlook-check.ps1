<#
.SYNOPSIS
  Manual Outlook compatibility check for a PST written by OST/PST Studio.

.DESCRIPTION
  This is the one check this project cannot automate honestly: whether classic Outlook
  opens a file without complaint. It is a MANUAL, OPT-IN workflow. It is never run by the
  test suite, and it is not run by the application.

  START CLASSIC OUTLOOK FIRST and leave its window open. The script attaches to the
  running instance. If it had to start Outlook itself, Outlook would run with no visible
  window and any prompt it raised (profile chooser, "file in use", a password request)
  would be invisible, and the script would appear to hang.

  What it does, step by step, with a prompt before anything that touches Outlook:
    1. Copies the PST to a scratch path, so Outlook never touches the original.
    2. Attaches the copy to the running Outlook profile with Namespace.AddStoreEx
       (the same thing File > Open > Outlook Data File does).
    3. Walks every folder and counts items, printing what Outlook sees.
    4. Detaches the store again with Namespace.RemoveStore.
  Adding and removing a store is a change to the Outlook profile for the duration of the
  check. Nothing is imported into any mailbox and no mail is sent.

  If Outlook refuses the file or asks for the Inbox Repair Tool (SCANPST), that is the
  result to record in docs/OUTLOOK_COMPATIBILITY.md.

.PARAMETER Path
  The PST to check.

.EXAMPLE
  .\scripts\outlook-check.ps1 -Path C:\converted\mailbox.pst
#>
param(
    [Parameter(Mandatory = $true)][string]$Path
)

$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Path)) { throw "no such file: $Path" }
$Path = (Resolve-Path $Path).Path

# Outlook must already be running with a window, or every prompt it raises is hidden.
$running = Get-Process OUTLOOK -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle }
if (-not $running) {
    Write-Host "Classic Outlook is not running with a visible window."
    Write-Host "Start Outlook (classic) normally, wait for it to finish loading, then run this again."
    Write-Host "New Outlook does not mount PST files and cannot be used for this check."
    exit 3
}
Write-Host "Found running Outlook: $($running[0].MainWindowTitle)"

$scratch = Join-Path $env:TEMP ("ostpst-outlook-check-" + [IO.Path]::GetFileName($Path))
Copy-Item -Force $Path $scratch
Write-Host "Copied to $scratch (Outlook will open the copy, never the original)."

$answer = Read-Host "Attach this copy to the running Outlook profile and read it back? [y/N]"
if ($answer -notmatch '^[Yy]') { Write-Host "Nothing done."; exit 0 }

Write-Host "Connecting to the running Outlook over COM..."
try {
    $outlook = [Runtime.InteropServices.Marshal]::GetActiveObject("Outlook.Application")
} catch {
    Write-Host "Could not attach to the running Outlook: $($_.Exception.Message)"
    Write-Host "If this terminal is elevated (Administrator) and Outlook is not, or the other way"
    Write-Host "round, COM cannot connect them. Run both at the same level."
    exit 3
}
$ns = $outlook.GetNamespace("MAPI")
Write-Host "Connected. Profile: $($ns.CurrentProfileName). Attaching the store (watch the Outlook window for any prompt)..."

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
Write-Host "Open a few messages there. If Outlook showed any error or asked for SCANPST, record it."

Read-Host "Press Enter to detach the store from the profile" | Out-Null
$ns.RemoveStore($store.GetRootFolder())
Write-Host "Detached. The profile is as it was. Scratch copy left at $scratch."
Write-Host "Result: Outlook opened the file and read $total item(s) without SCANPST."
exit 0
