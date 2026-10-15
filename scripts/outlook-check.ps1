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

  What the script does, step by step:
    1. Copies the PST to a scratch path, so Outlook never touches the original.
    2. In a background job (an interactive console can deadlock inside Outlook's COM
       calls), attaches the copy with Namespace.AddStoreEx, the same thing File > Open
       > Outlook Data File does; walks every folder and counts items; resolves the
       special folders (Inbox, Calendar, Contacts, Tasks, Notes, Journal); reads the
       first message; then detaches the store with Namespace.RemoveStore.
    3. Prints what Outlook saw, or that it refused the file, or that it did not answer
       within the time limit.

  If Outlook refuses the file, asks for the Inbox Repair Tool (SCANPST), or a folder
  never finishes loading, that is the result to record in docs/OUTLOOK_COMPATIBILITY.md.

.PARAMETER Path
  The PST to check.

.PARAMETER TimeoutSeconds
  How long to wait for Outlook before giving up (default 120).

.PARAMETER Detach
  Only detach any store whose file name contains this file's name, for cleaning up
  after a run that did not finish.

.EXAMPLE
  .\scripts\outlook-check.ps1 -Path C:\converted\mailbox.pst
#>
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [int]$TimeoutSeconds = 120,
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

# Everything that talks to Outlook runs in a job. Outlook is a single-instance COM
# server, so the job's New-Object connects to the running copy. The job gives the call
# a time limit and keeps the interactive console out of the COM conversation.
$probe = Start-Job { $ol = New-Object -ComObject Outlook.Application; $ol.GetNamespace("MAPI").CurrentProfileName }
if (-not (Wait-Job $probe -Timeout 30)) { Remove-Job $probe -Force; Write-Host "Outlook did not answer a COM call within 30 s."; exit 3 }
$profile = Receive-Job $probe; Remove-Job $probe
Write-Host "Connected. Profile: $profile."
if ($profile -ne "OSTPST-Test") {
    Write-Host "WARNING: this is not the throwaway profile. A file Outlook cannot load will"
    Write-Host "leave this profile stuck until the store is removed again."
    $go = Read-Host "Continue with profile '$profile' anyway? [y/N]"
    if ($go -notmatch '^[Yy]') { Write-Host "Nothing done."; exit 0 }
}

if ($Detach) {
    $name = [IO.Path]::GetFileName($Path)
    $job = Start-Job -ArgumentList $name {
        param($name)
        $ol = New-Object -ComObject Outlook.Application; $ns = $ol.GetNamespace("MAPI")
        $found = $false
        foreach ($s in @($ns.Stores)) {
            if ($s.FilePath -and ([IO.Path]::GetFileName($s.FilePath) -like "*$name*")) {
                "Detaching $($s.FilePath)"; $ns.RemoveStore($s.GetRootFolder()); $found = $true
            }
        }
        if (-not $found) { "No attached store matches '$name'." }
    }
    if (Wait-Job $job -Timeout $TimeoutSeconds) { Receive-Job $job } else { Write-Host "Outlook did not finish detaching within $TimeoutSeconds s." }
    Remove-Job $job -Force -ErrorAction SilentlyContinue
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

Write-Host "Attaching the store (watch the Outlook window for any prompt; up to $TimeoutSeconds s)..."
$job = Start-Job -ArgumentList $scratch {
    param($f)
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $ol = New-Object -ComObject Outlook.Application
    $ns = $ol.GetNamespace("MAPI")
    try {
        $ns.AddStoreEx($f, 3)   # olStoreUnicode
    } catch {
        "REFUSED after $([int]$sw.Elapsed.TotalSeconds) s: $($_.Exception.Message)"
        "RESULT: NOT compatible as written. Note whether Outlook asked for SCANPST."
        return
    }
    "Attached in $([int]$sw.Elapsed.TotalSeconds) s."
    $store = $ns.Stores | Where-Object { $_.FilePath -eq $f } | Select-Object -First 1
    if (-not $store) { "The store was added but is not listed; nothing more can be read."; return }
    function Walk($folder, $depth) {
        $count = -1
        try { $count = $folder.Items.Count } catch {}
        ("{0}{1}  ({2} items)" -f ('  ' * $depth), $folder.Name, $count)
        $total = [Math]::Max(0, $count)
        foreach ($sub in $folder.Folders) { $total += Walk $sub ($depth + 1) }
        return $total
    }
    "Outlook sees:"
    $lines = @(Walk $store.GetRootFolder() 0)
    $total = $lines[-1]; $lines[0..($lines.Count - 2)]
    "Total items visible to Outlook: $total"
    # olFolderInbox 6, Calendar 9, Contacts 10, Journal 11, Notes 12, Tasks 13, Drafts 16
    foreach ($pair in @(@(6,"Inbox"),@(9,"Calendar"),@(10,"Contacts"),@(13,"Tasks"),@(12,"Notes"),@(11,"Journal"),@(16,"Drafts"))) {
        try { $d = $store.GetDefaultFolder($pair[0]); "Special folder $($pair[1]) -> $($d.FolderPath) ($($d.Items.Count) items)" }
        catch { "Special folder $($pair[1]) -> not resolved: $($_.Exception.Message)" }
    }
    try {
        $inbox = $store.GetDefaultFolder(6)
        $m = $inbox.Items | Select-Object -First 1
        if ($m) { "First Inbox message: '$($m.Subject)' from '$($m.SenderName)', body: " + (($m.Body -replace '\s+', ' ').Substring(0, [Math]::Min(80, $m.Body.Length))) }
    } catch { "Reading a message failed: $($_.Exception.Message)" }
    $ns.RemoveStore($store.GetRootFolder())
    "Detached. The profile is as it was."
    "RESULT: Outlook attached the file, listed $total item(s) and detached it without asking for SCANPST."
}
if (Wait-Job $job -Timeout $TimeoutSeconds) {
    Receive-Job $job
} else {
    Write-Host "Outlook did not finish within $TimeoutSeconds s. Look at the Outlook window: a prompt"
    Write-Host "(SCANPST, password) means Outlook needs the file repaired first; a spinning folder"
    Write-Host "means it could not load the store. Close Outlook, start it again with the"
    Write-Host "throwaway profile, and run this script with -Detach to clean up."
    Stop-Job $job -ErrorAction SilentlyContinue
}
Remove-Job $job -Force -ErrorAction SilentlyContinue
Write-Host "Scratch copy left at $scratch."
