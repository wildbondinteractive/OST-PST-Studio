# Test fixtures are real Outlook files, so they are never committed. These three are the
# public fixtures from hrbrmstr/freepst (a PST, an Outlook 2013 OST, and a PST with a
# "password" that protects nothing). Tests skip rather than fail when they are absent.
#
#   .\scripts\fetch-fixtures.ps1
$dir = Join-Path (Split-Path $PSScriptRoot -Parent) 'fixtures\data'
New-Item -ItemType Directory -Force $dir | Out-Null
foreach ($f in 'dist-list.pst', 'example-2013.ost', 'passworded.pst') {
    $target = Join-Path $dir $f
    if (Test-Path $target) { Write-Host "already have $f"; continue }
    Invoke-WebRequest "https://github.com/hrbrmstr/freepst/raw/master/inst/extdata/$f" -OutFile $target
}
Get-ChildItem $dir | Select-Object Name, Length
