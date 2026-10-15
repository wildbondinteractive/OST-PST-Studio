<#
.SYNOPSIS
  Verify the Authenticode signatures on OST/PST Studio release files.

.DESCRIPTION
  For every file named (or every file in a directory): the file exists, carries a
  signature whose status is Valid, was signed with a certificate that has the
  code-signing usage and chains to a trusted root, used a SHA-256 file digest, carries a
  trusted RFC 3161 timestamp, and, when asked, names the expected signer. With
  -Embedded, the NSIS installer and the MSI are unpacked and the OST/PST Studio
  executable inside each is put through the same checks, so an installer signed on the
  outside around an unsigned program cannot pass.

  Exit code 0 only when every check on every file passes. Never skips a file.

  Used by .github/workflows/release.yml before anything is published; usable by anyone
  on a downloaded release, in Windows PowerShell 5.1 or PowerShell 7. Needs signtool.exe
  (Windows SDK) for the digest check, 7-Zip for unpacking the NSIS installer and msiexec
  for the MSI.

.PARAMETER Path
  Files to verify, or a directory whose .exe and .msi files are verified.

.PARAMETER ExpectedSubject
  Text the signer certificate's subject must contain (for example the publisher's CN).
  Empty means "report the subject but do not check it".

.PARAMETER ExpectedRoot
  Text the chain's root certificate subject must contain. Defaults to the root of the
  Azure Artifact Signing public trust chain.

.PARAMETER Embedded
  Also unpack installers and verify the executable inside.

.PARAMETER ExpectedVersion
  Product version every OST/PST Studio executable (including the ones inside the
  installers) must carry in its version resource. Empty means "do not check".

.PARAMETER ExpectedEmbeddedHash
  SHA-256 the executable inside each installer must have. Normally left empty: the
  Tauri bundler writes a bundle-type marker into the executable it packages, so the
  file inside an installer legitimately differs from the portable executable by a few
  bytes and by its own signature. Its signature, not its hash, is what is verified.

.EXAMPLE
  .\scripts\verify-signatures.ps1 -Path release -Embedded
  .\scripts\verify-signatures.ps1 -Path "$env:USERPROFILE\Downloads\OST-PST Studio_0.1.2_x64-setup.exe" -Embedded
#>
#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string[]]$Path,
    [string]$ExpectedSubject = "",
    [string]$ExpectedRoot = "Microsoft Identity Verification Root Certificate Authority 2020",
    [switch]$Embedded,
    [string]$ExpectedVersion = "",
    [string]$ExpectedEmbeddedHash = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$CodeSigningEku = "1.3.6.1.5.5.7.3.3"

function Find-Tool {
    param([string]$Name, [string[]]$Candidates)
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($c in $Candidates) {
        $hit = Get-ChildItem $c -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
        if ($hit) { return $hit.FullName }
    }
    return $null
}

$signtool = Find-Tool "signtool.exe" @(
    "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe",
    "$env:ProgramFiles\Windows Kits\10\bin\*\x64\signtool.exe"
)
$sevenZip = Find-Tool "7z.exe" @("$env:ProgramFiles\7-Zip\7z.exe", "${env:ProgramFiles(x86)}\7-Zip\7z.exe")

# Every check on one file. Returns a list of [pscustomobject] rows with Check, Pass, Detail.
function Test-SignedFile {
    param([string]$File, [string]$Label)
    $rows = New-Object System.Collections.Generic.List[object]
    function Add-Row([string]$check, [bool]$pass, [string]$detail) {
        $rows.Add([pscustomobject]@{ File = $Label; Check = $check; Pass = $pass; Detail = $detail })
    }

    if (-not (Test-Path -LiteralPath $File -PathType Leaf)) {
        Add-Row "exists" $false "missing"
        return $rows
    }
    Add-Row "exists" $true ("{0:N0} bytes" -f (Get-Item -LiteralPath $File).Length)

    if (-not [string]::IsNullOrWhiteSpace($ExpectedVersion) -and $File -like "*.exe") {
        $pv = (Get-Item -LiteralPath $File).VersionInfo.ProductVersion
        Add-Row "version resource" ($null -ne $pv -and $pv.StartsWith($ExpectedVersion)) "product version '$pv'"
    }

    $sig = Get-AuthenticodeSignature -LiteralPath $File
    Add-Row "signed" ($sig.Status -ne "NotSigned" -and $null -ne $sig.SignerCertificate) "status $($sig.Status)"
    if ($null -eq $sig.SignerCertificate) {
        return $rows
    }
    Add-Row "signature valid" ($sig.Status -eq "Valid") "$($sig.Status): $($sig.StatusMessage)"

    $cert = $sig.SignerCertificate
    $subject = $cert.Subject
    $cn = ($subject -split ",\s*" | Where-Object { $_ -like "CN=*" } | Select-Object -First 1)
    $subjectOk = [string]::IsNullOrWhiteSpace($ExpectedSubject) -or $subject.Contains($ExpectedSubject)
    Add-Row "signer" $subjectOk "$cn (issuer $(($cert.Issuer -split ',\s*')[0]), expires $($cert.NotAfter.ToString('u')))"

    $ekus = @($cert.EnhancedKeyUsageList | ForEach-Object { $_.ObjectId })
    Add-Row "code-signing usage" ($ekus -contains $CodeSigningEku) ($ekus -join ", ")

    # The chain is built to name the root; validity over time is Get-AuthenticodeSignature's
    # job, which honours the timestamp, so an expired (short-lived) signing certificate
    # is fine here.
    $chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()
    $chain.ChainPolicy.VerificationFlags = [System.Security.Cryptography.X509Certificates.X509VerificationFlags]::IgnoreNotTimeValid
    $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::NoCheck
    [void]$chain.Build($cert)
    $root = $chain.ChainElements[$chain.ChainElements.Count - 1].Certificate
    $rootCn = (($root.Subject -split ",\s*") | Where-Object { $_ -like "CN=*" } | Select-Object -First 1)
    $rootOk = [string]::IsNullOrWhiteSpace($ExpectedRoot) -or $root.Subject.Contains($ExpectedRoot)
    Add-Row "trusted root" ($rootOk -and $sig.Status -eq "Valid") "$rootCn, $($chain.ChainElements.Count) certificates in chain"

    $ts = $sig.TimeStamperCertificate
    if ($null -ne $ts) {
        $tsCn = (($ts.Subject -split ",\s*") | Where-Object { $_ -like "CN=*" } | Select-Object -First 1)
        Add-Row "timestamped" $true "by $tsCn"
    } else {
        Add-Row "timestamped" $false "no RFC 3161 timestamp; the signature would die with the certificate"
    }

    if ($signtool) {
        $out = & $signtool verify /pa /all /v $File 2>&1 | Out-String
        $verified = $LASTEXITCODE -eq 0
        $digest = if ($out -match "Hash of file \((\w+)\)") { $Matches[1].ToLower() } else { "unknown" }
        Add-Row "signtool verify /pa" $verified ($(if ($verified) { "successfully verified" } else { ($out -split "`n" | Select-Object -Last 3) -join " " }))
        Add-Row "SHA-256 digest" ($digest -eq "sha256") "file digest $digest"
        Add-Row "timestamp verified by signtool" ($out -match "The signature is timestamped") ($(if ($out -match "The signature is timestamped:\s*(.+)") { $Matches[1].Trim() } else { "no timestamp line" }))
    } else {
        Add-Row "signtool verify /pa" $false "signtool.exe not found; install the Windows SDK signing tools"
    }
    return $rows
}

function Get-EmbeddedExecutable {
    param([string]$Installer)
    $work = Join-Path ([System.IO.Path]::GetTempPath()) ("ostpst-verify-" + [guid]::NewGuid().ToString("n"))
    New-Item -ItemType Directory -Path $work | Out-Null
    $ext = [System.IO.Path]::GetExtension($Installer).ToLower()
    if ($ext -eq ".msi") {
        $p = Start-Process msiexec.exe -ArgumentList "/a", "`"$Installer`"", "/qn", "TARGETDIR=`"$work`"" -Wait -PassThru
        if ($p.ExitCode -ne 0) { throw "msiexec /a failed with $($p.ExitCode) on $Installer" }
    } elseif ($ext -eq ".exe") {
        if (-not $sevenZip) { throw "7z.exe not found; needed to unpack the NSIS installer" }
        & $sevenZip x -y "-o$work" $Installer | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "7-Zip could not unpack $Installer" }
    } else {
        throw "cannot unpack $Installer"
    }
    $exe = Get-ChildItem $work -Recurse -File -Filter "ostpst-studio.exe" | Select-Object -First 1
    if (-not $exe) { throw "no ostpst-studio.exe inside $Installer" }
    return $exe.FullName
}

# ---- Collect the files -----------------------------------------------------------------
$files = New-Object System.Collections.Generic.List[string]
foreach ($p in $Path) {
    if (Test-Path -LiteralPath $p -PathType Container) {
        Get-ChildItem -LiteralPath $p -File | Where-Object { $_.Extension -in ".exe", ".msi" } | Sort-Object Name | ForEach-Object { $files.Add($_.FullName) }
    } else {
        $resolved = Resolve-Path -LiteralPath $p -ErrorAction SilentlyContinue
        $files.Add($(if ($resolved) { $resolved.Path } else { $p }))
    }
}
if ($files.Count -eq 0) { throw "nothing to verify under: $($Path -join ', ')" }

$all = New-Object System.Collections.Generic.List[object]
foreach ($f in $files) {
    $label = [System.IO.Path]::GetFileName($f)
    foreach ($r in (Test-SignedFile $f $label)) { $all.Add($r) }
    $isInstaller = $label -like "*-setup.exe" -or $label -like "*.msi"
    if ($Embedded -and $isInstaller) {
        try {
            $inner = Get-EmbeddedExecutable $f
            $innerLabel = "$label -> ostpst-studio.exe"
            foreach ($r in (Test-SignedFile $inner $innerLabel)) { $all.Add($r) }
            if (-not [string]::IsNullOrWhiteSpace($ExpectedEmbeddedHash)) {
                $h = (Get-FileHash -LiteralPath $inner -Algorithm SHA256).Hash
                $all.Add([pscustomobject]@{ File = $innerLabel; Check = "same bytes as the signed executable"; Pass = ($h -eq $ExpectedEmbeddedHash.ToUpper()); Detail = $h.ToLower() })
            }
            Remove-Item -Recurse -Force (Split-Path $inner -Parent) -ErrorAction SilentlyContinue
        } catch {
            $all.Add([pscustomobject]@{ File = $label; Check = "embedded executable"; Pass = $false; Detail = "$_" })
        }
    }
}

# ---- Report ----------------------------------------------------------------------------
$failed = @($all | Where-Object { -not $_.Pass })
$table = $all | ForEach-Object { "{0}  {1,-46} {2,-38} {3}" -f ($(if ($_.Pass) { "PASS" } else { "FAIL" }), $_.File, $_.Check, $_.Detail) }
$table | ForEach-Object { Write-Host $_ }
$summary = "$($all.Count) checks on $($files.Count) file(s): $($all.Count - $failed.Count) passed, $($failed.Count) failed"
Write-Host ""
Write-Host $summary

if ($env:GITHUB_STEP_SUMMARY) {
    $md = @("## Signature verification", "", $summary, "", "| Result | File | Check | Detail |", "|---|---|---|---|")
    $md += $all | ForEach-Object { "| $(if ($_.Pass) { 'PASS' } else { 'FAIL' }) | $($_.File) | $($_.Check) | $(($_.Detail -replace '\|', '/')) |" }
    ($md -join "`n") + "`n" | Add-Content -Path $env:GITHUB_STEP_SUMMARY
}

if ($failed.Count -gt 0) {
    Write-Error "signature verification failed: $($failed.Count) check(s) did not pass" -ErrorAction Continue
    exit 1
}
exit 0
