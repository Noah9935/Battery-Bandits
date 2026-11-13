# Überträgt den aktuell ausgecheckten Stand (letzter Commit) auf den Schulserver und startet die Umgebung neu.
#
#   powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 staging              Images auf dem Server bauen
#   powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 staging -LocalImages lokal gebaute Images übertragen (schneller)
#   powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 prod
#   powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 banner               nur die Seite "In Bearbeitung" auf der Prod-Domain
#
# Voraussetzung: SSH-Alias "bb" und auf dem Server eine ausgefüllte .env im Umgebungsordner
# (Vorlage: deploy/.env.staging.example oder deploy/.env.prod.example, siehe docs/deployment.md).
# Mit -LocalImages müssen die Images vorher lokal gebaut sein:
#   docker compose -f deploy/docker-compose.yml --env-file deploy/.env.local.example --profile local build
param(
    [Parameter(Mandatory)]
    [ValidateSet('staging', 'prod', 'banner')]
    [string]$Target,
    [switch]$LocalImages,
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
$envName = if ($Target -eq 'banner') { 'prod' } else { $Target }
$dir = "~/battery-bandits/$envName"
$archive = Join-Path ([System.IO.Path]::GetTempPath()) "battery-bandits-$Target.tar.gz"
$images = 'web', 'ingestion-service', 'monitoring-service', 'operations-service', 'dashboard-service' | ForEach-Object { "battery-bandits/${_}:latest" }
$imageFile = Join-Path ([System.IO.Path]::GetTempPath()) 'battery-bandits-images.tar'

Push-Location (Join-Path $PSScriptRoot '..')
try {
    git archive --format=tar.gz -o $archive HEAD
    if ($LASTEXITCODE -ne 0) { throw 'git archive fehlgeschlagen' }

    ssh bb "mkdir -p $dir/src && rm -rf $dir/src/* && test -f $dir/.env || { echo 'Auf dem Server fehlt $dir/.env (siehe docs/deployment.md)'; exit 1; }"
    if ($LASTEXITCODE -ne 0) { throw 'Vorbereitung auf dem Server fehlgeschlagen' }

    scp $archive "bb:$dir/src.tar.gz"
    if ($LASTEXITCODE -ne 0) { throw 'Upload fehlgeschlagen' }
    ssh bb "cd $dir && tar -xzf src.tar.gz -C src && rm src.tar.gz"
    if ($LASTEXITCODE -ne 0) { throw 'Entpacken fehlgeschlagen' }

    if ($Target -eq 'banner') {
        # Läuft auf Prod die App, belegt sie denselben Port: erst beenden (Projekt bb-prod), dann die Seite starten.
        ssh bb "cd $dir/src/deploy && docker compose --env-file ../../.env down --remove-orphans; cd coming-soon && docker compose --env-file ../../../.env up -d --force-recreate && docker compose --env-file ../../../.env ps"
    }
    elseif ($NoBuild) {
        ssh bb "cd $dir/src/deploy && docker compose --env-file ../../.env up -d --no-build && docker compose --env-file ../../.env ps"
    }
    elseif ($LocalImages) {
        docker save -o $imageFile @images
        if ($LASTEXITCODE -ne 0) { throw 'docker save fehlgeschlagen (Images lokal gebaut?)' }
        scp $imageFile "bb:$dir/images.tar"
        if ($LASTEXITCODE -ne 0) { throw 'Upload der Images fehlgeschlagen' }
        ssh bb "cd $dir && docker load -i images.tar && rm images.tar && cd src/deploy && docker compose --env-file ../../.env up -d --no-build && docker compose --env-file ../../.env ps"
    }
    else {
        ssh bb "cd $dir/src/deploy && docker compose --env-file ../../.env up -d --build && docker compose --env-file ../../.env ps"
    }
    if ($LASTEXITCODE -ne 0) { throw 'Start fehlgeschlagen' }
}
finally {
    Remove-Item -ErrorAction SilentlyContinue $archive, $imageFile
    Pop-Location
}
