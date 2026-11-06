# Startet alles lokal: Datenbank, Mosquitto, vier Dienste, Simulator und Weboberfläche.
# Benötigt nur Docker. Danach im Browser: http://localhost:8080
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    docker compose --env-file .env.local.example --profile local up --build -d @args
    if ($LASTEXITCODE -ne 0) { throw 'docker compose ist fehlgeschlagen' }
    Write-Host ''
    Write-Host 'Battery Bandits läuft: http://localhost:8080'
    Write-Host 'Protokolle:  docker compose --env-file .env.local.example --profile local logs -f'
    Write-Host 'Stoppen:     docker compose --env-file .env.local.example --profile local down'
}
finally {
    Pop-Location
}
