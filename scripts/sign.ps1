<#
.SYNOPSIS
  Authenticode-sign one file with the code-signing certificate on this PC.

.DESCRIPTION
  Called by the Tauri bundler for every executable and installer when you build with
  the signed configuration:

      npm run build:signed

  The certificate is found in the current user's personal store. Set
  OSTPST_SIGN_THUMBPRINT to choose one when several are installed. A timestamp is
  always applied so the signature stays valid after the certificate expires.

  Nothing here is secret: the private key never leaves the Windows certificate store
  (or the hardware token or cloud HSM it lives on) and signtool uses it in place.
#>
param([Parameter(Mandatory = $true)][string]$File)

$ErrorActionPreference = 'Stop'

$signtool = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $signtool) { throw "signtool.exe not found; install the Windows SDK (Signing Tools) component" }

$thumb = $env:OSTPST_SIGN_THUMBPRINT
if (-not $thumb) {
    $certs = @(Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Where-Object { $_.NotAfter -gt (Get-Date) })
    if ($certs.Count -eq 0) { throw "no code-signing certificate in the user store; see docs/RELEASING.md" }
    if ($certs.Count -gt 1) { throw "several code-signing certificates found; set OSTPST_SIGN_THUMBPRINT to one of: $($certs.Thumbprint -join ', ')" }
    $thumb = $certs[0].Thumbprint
}

$timestamp = if ($env:OSTPST_TIMESTAMP_URL) { $env:OSTPST_TIMESTAMP_URL } else { "http://timestamp.digicert.com" }

& $signtool sign /fd sha256 /td sha256 /tr $timestamp /sha1 $thumb /d "OST/PST Studio" $File
if ($LASTEXITCODE -ne 0) { throw "signtool failed for $File" }
& $signtool verify /pa /q $File
if ($LASTEXITCODE -ne 0) { throw "signature on $File does not verify" }
Write-Host "signed $File"
