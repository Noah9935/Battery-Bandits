# Überträgt den aktuell ausgecheckten Stand (letzter Commit) auf den Schulserver und startet die Umgebung neu.
# Die Images werden auf dem Server gebaut, dafür muss nichts gepusht werden.
#
#   powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 staging
#   powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 prod
#
# Voraussetzung: SSH-Alias "bb" und auf dem Server eine ausgefüllte .env im Umgebungsordner
# (Vorlage: deploy/.env.staging.example oder deploy/.env.prod.example, siehe docs/deployment.md).
param(
    [Parameter(Mandatory)]
    [ValidateSet('staging', 'prod')]
    [string]$Target
)

$ErrorActionPreference = 'Stop'
$dir = "~/battery-bandits/$Target"
$archive = Join-Path ([System.IO.Path]::GetTempPath()) "battery-bandits-$Target.tar.gz"

Push-Location (Join-Path $PSScriptRoot '..')
try {
    git archive --format=tar.gz -o $archive HEAD
    if ($LASTEXITCODE -ne 0) { throw 'git archive fehlgeschlagen' }

    ssh bb "mkdir -p $dir/src && rm -rf $dir/src/* && test -f $dir/.env || { echo 'Auf dem Server fehlt $dir/.env (siehe docs/deployment.md)'; exit 1; }"
    if ($LASTEXITCODE -ne 0) { throw 'Vorbereitung auf dem Server fehlgeschlagen' }

    scp $archive "bb:$dir/src.tar.gz"
    if ($LASTEXITCODE -ne 0) { throw 'Upload fehlgeschlagen' }

    ssh bb "cd $dir && tar -xzf src.tar.gz -C src && rm src.tar.gz && cd src/deploy && docker compose --env-file ../../.env up -d --build && docker compose --env-file ../../.env ps"
    if ($LASTEXITCODE -ne 0) { throw 'Start fehlgeschlagen' }
}
finally {
    Remove-Item -ErrorAction SilentlyContinue $archive
    Pop-Location
}
