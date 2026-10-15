<#
.SYNOPSIS
  Prove, locally and without Azure, the packaging sequence the release workflow relies on.

.DESCRIPTION
  The release workflow builds the executable, then runs `tauri bundle` with a sign
  command. This script does the same with scripts/dev/log-sign-command.ps1 standing in
  for the real sign command, and checks what the workflow depends on:

    - `tauri bundle` does not rebuild the executable (same hash before and after).
    - The bundler hands the sign command the executable once per installer, carrying
      that installer's bundle-type marker (NSS, then MSI), before it hands over the
      installer itself; so signing at that moment signs what the installer embeds.
    - The executable unpacked from each installer carries the matching marker.
    - Afterwards the executable on disk is back to its plain (UNK) state, which is
      what the portable build is made from.

  Nothing is signed here; the signatures are the workflow's job.
#>
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")

npm run tauri build -- --ci --no-bundle
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
$before = (Get-FileHash target\release\ostpst-studio.exe -Algorithm SHA256).Hash
"executable before bundling: $before"

$log = "scripts\dev\sign-command.log"
Remove-Item $log -ErrorAction SilentlyContinue
$cfg = '{"bundle":{"windows":{"signCommand":"pwsh -NoProfile -ExecutionPolicy Bypass -File ../scripts/dev/log-sign-command.ps1 %1"}}}'
npx tauri bundle --bundles nsis,msi --config $cfg
if ($LASTEXITCODE -ne 0) { throw "tauri bundle failed" }

$after = (Get-FileHash target\release\ostpst-studio.exe -Algorithm SHA256).Hash
if ($after -ne $before) { throw "tauri bundle left the executable changed ($after)" }
"executable after bundling:  $after (unchanged)"

$calls = Get-Content $log
"sign command was called $($calls.Count) times:"
$calls | ForEach-Object { "  $_" }
$exeCalls = @($calls | Where-Object { $_ -match "file=.*\\ostpst-studio\.exe size=\d+ marker=(\w+)" } | ForEach-Object { $Matches[1] })
if (($exeCalls -join ",") -ne "NSS,MSI") { throw "expected the executable to be offered with markers NSS then MSI, got: $($exeCalls -join ',')" }
$order = @($calls | ForEach-Object {
    if ($_ -match "ostpst-studio\.exe .*marker=(\w+)") { "exe:$($Matches[1])" }
    elseif ($_ -match "-setup\.exe ") { "nsis-installer" }
    elseif ($_ -match "\.msi ") { "msi-installer" }
    elseif ($_ -match "\.tmp ") { "nsis-uninstaller-stub" }
})
$want = "exe:NSS", "nsis-uninstaller-stub", "nsis-installer", "exe:MSI", "msi-installer"
if (($order -join ",") -ne ($want -join ",")) { throw "unexpected sign order: $($order -join ', ') (wanted $($want -join ', '))" }
"sign order: $($order -join ' > ')"

$work = Join-Path ([System.IO.Path]::GetTempPath()) "ostpst-staged-check"
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory $work | Out-Null
$nsis = Get-ChildItem target\release\bundle\nsis\*-setup.exe | Select-Object -First 1
$msi = Get-ChildItem target\release\bundle\msi\*.msi | Select-Object -First 1
$sevenZip = if (Get-Command 7z.exe -ErrorAction SilentlyContinue) { "7z.exe" } else { "$env:ProgramFiles\7-Zip\7z.exe" }
& $sevenZip x -y "-o$work\nsis" $nsis.FullName | Out-Null
if ($LASTEXITCODE -ne 0) { throw "7-Zip could not unpack the NSIS installer" }
$p = Start-Process msiexec.exe -ArgumentList "/a", "`"$($msi.FullName)`"", "/qn", "TARGETDIR=`"$work\msi`"" -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "msiexec /a failed with $($p.ExitCode)" }

Remove-Item $log
foreach ($pair in @(@("nsis", "NSS"), @("msi", "MSI"))) {
    $kind, $marker = $pair
    $inner = Get-ChildItem "$work\$kind" -Recurse -File -Filter ostpst-studio.exe | Select-Object -First 1
    if (-not $inner) { throw "no ostpst-studio.exe inside the $kind installer" }
    pwsh -NoProfile -File scripts\dev\log-sign-command.ps1 $inner.FullName | Out-Null
    $line = Get-Content $log | Select-Object -Last 1
    if ($line -notmatch "marker=$marker$") { throw "the executable inside the $kind installer does not carry marker $marker ($line)" }
    "$kind installer carries the executable with marker $marker"
}
Remove-Item $log -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $work
"staged packaging check passed"
