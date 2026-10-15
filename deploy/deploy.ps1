# Lädt deploy/ auf den Server und startet die Umgebung neu.
# Aufruf aus dem Repo-Ordner: powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 staging
param(
    [Parameter(Mandatory)]
    [ValidateSet('staging', 'prod')]
    [string]$Target
)

$ErrorActionPreference = 'Stop'
$dir = "~/battery-bandits/$Target"

Push-Location $PSScriptRoot
try {
    ssh bb "mkdir -p $dir"
    if ($LASTEXITCODE -ne 0) { throw 'SSH fehlgeschlagen' }

    scp -r docker-compose.yml Caddyfile .env.example www "bb:$dir/"
    if ($LASTEXITCODE -ne 0) { throw 'Upload fehlgeschlagen' }

    ssh bb "cd $dir && test -f .env || { echo '.env fehlt in $dir (siehe docs/deployment.md)'; exit 1; } && docker compose up -d && docker compose ps"
    if ($LASTEXITCODE -ne 0) { throw 'Start fehlgeschlagen' }
}
finally {
    Pop-Location
}
