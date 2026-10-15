<#
.SYNOPSIS
  Sign one file with Azure Artifact Signing. The Tauri bundler's sign command.

.DESCRIPTION
  Called by the Tauri bundler for every file it wants signed when it is run with
  src-tauri/tauri.release.conf.json: the application executable (once per installer,
  after the bundler has written that installer's bundle-type marker into it) and then
  each finished installer. The release workflow also calls it for the portable
  executable and the command-line tool.

  Signs with the ArtifactSigning PowerShell module, the same component the official
  azure/artifact-signing-action uses, so the result is identical to that action's:
  SHA-256 digest, RFC 3161 timestamp from Microsoft's timestamp service with a SHA-256
  timestamp digest. Authentication is DefaultAzureCredential restricted to the Azure CLI
  session that azure/login established with OIDC; every other credential type is
  excluded, so there is nothing a stray environment variable could add.

  Needs, from the environment (set by the release workflow from the `release-signing`
  environment's variables; none of them is a secret):
    AZURE_SIGNING_ENDPOINT, AZURE_SIGNING_ACCOUNT, AZURE_CERT_PROFILE

  Fails, and so fails the bundle, if anything is missing, if signing fails, or if the
  signature does not verify afterwards. There is no unsigned fallback.
#>
param([Parameter(Mandatory = $true)][string]$File)

$ErrorActionPreference = "Stop"

foreach ($name in "AZURE_SIGNING_ENDPOINT", "AZURE_SIGNING_ACCOUNT", "AZURE_CERT_PROFILE") {
    if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
        throw "azure-sign.ps1: $name is not set; signing is only possible in the release workflow (see docs/AZURE_CODE_SIGNING.md)"
    }
}
$File = (Resolve-Path -LiteralPath $File).Path
if (-not (Test-Path -LiteralPath $File -PathType Leaf)) { throw "azure-sign.ps1: no such file: $File" }

# The bundler also offers the NSIS plug-in DLLs it embeds and the WiX extension DLLs it
# builds with. Those are third-party tools, not this project's code, and are not signed
# under this project's identity: the installer's own signature covers everything inside
# it. What is signed: the application executable (once per installer, carrying that
# installer's marker), the NSIS uninstaller stub (handed over as a .tmp file by the
# installer's finalize hook), and each finished installer.
$ext = [System.IO.Path]::GetExtension($File).ToLowerInvariant()
if ($ext -notin ".exe", ".msi", ".tmp") {
    Write-Host "azure-sign.ps1: not signing third-party file $File"
    exit 0
}

Import-Module ArtifactSigning -ErrorAction Stop

$params = @{
    Endpoint                           = $env:AZURE_SIGNING_ENDPOINT
    CodeSigningAccountName             = $env:AZURE_SIGNING_ACCOUNT
    CertificateProfileName             = $env:AZURE_CERT_PROFILE
    Files                              = $File
    FileDigest                         = "SHA256"
    TimestampRfc3161                   = "http://timestamp.acs.microsoft.com"
    TimestampDigest                    = "SHA256"
    Description                        = "OST/PST Studio"
    DescriptionUrl                     = "https://github.com/wildbondinteractive/OST-PST-Studio"
    ExcludeEnvironmentCredential       = $true
    ExcludeWorkloadIdentityCredential  = $true
    ExcludeManagedIdentityCredential   = $true
    ExcludeSharedTokenCacheCredential  = $true
    ExcludeVisualStudioCredential      = $true
    ExcludeVisualStudioCodeCredential  = $true
    ExcludeAzureCliCredential          = $false
    ExcludeAzurePowerShellCredential   = $true
    ExcludeAzureDeveloperCliCredential = $true
    ExcludeInteractiveBrowserCredential = $true
}
Write-Host "azure-sign.ps1: signing $File"
Invoke-ArtifactSigning @params

$sig = Get-AuthenticodeSignature -LiteralPath $File
if ($sig.Status -ne "Valid") { throw "azure-sign.ps1: $File is not validly signed after signing: $($sig.Status) $($sig.StatusMessage)" }
if ($null -eq $sig.TimeStamperCertificate) { throw "azure-sign.ps1: $File was signed without a timestamp" }
Write-Host "azure-sign.ps1: signed $File as '$($sig.SignerCertificate.Subject)', timestamped by '$($sig.TimeStamperCertificate.Subject)'"
