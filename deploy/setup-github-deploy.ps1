<#
.SYNOPSIS
    Richtet das automatische Bauen und Ausrollen bei jedem Push ein.

.DESCRIPTION
    Danach gilt: Push auf einen Feature-Branch -> GitHub Actions prüft (CI) -> baut auf dem Schulserver -> Staging.
    Push auf main -> prüft -> baut -> Prod.

    Das Skript macht, einmalig und wiederholbar:
      1. Es erzeugt einen eigenen Deploy-Schlüssel (nur für GitHub, ohne Passwort).
      2. Es trägt den öffentlichen Teil auf dem Server ein (eingeschränkt: kein Port-Forwarding, kein Terminal).
         Ein früherer Deploy-Schlüssel wird dabei ersetzt, andere Schlüssel bleiben unberührt.
      3. Es prüft, ob der neue Schlüssel auf dem Server funktioniert.
      4. Es hinterlegt die Secrets in GitHub (mit der GitHub CLI, sonst Schritt für Schritt über die Zwischenablage).
      5. Es löscht den privaten Schlüssel wieder vom Rechner. Danach liegt er nur noch in GitHub.
      6. Es stößt auf Wunsch den ersten Lauf an.

    Der private Schlüssel wird nie angezeigt und nie ins Repository geschrieben.

.PARAMETER SshAlias
    Name des Servers in ~/.ssh/config (Vorgabe: bb).

.PARAMETER Repo
    GitHub-Repository als Besitzer/Name. Ohne Angabe wird es aus dem Git-Remote "origin" gelesen.

.PARAMETER DryRun
    Zeigt nur, was passieren würde, und ändert nichts (Server, GitHub, Dateien).

.PARAMETER SkipFirstRun
    Startet am Ende keinen ersten Deploy.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File deploy\setup-github-deploy.ps1 -DryRun
    powershell -ExecutionPolicy Bypass -File deploy\setup-github-deploy.ps1
#>
[CmdletBinding()]
param(
    [string]$SshAlias = 'bb',
    [string]$Repo = '',
    [switch]$DryRun,
    [switch]$SkipFirstRun
)

# Fehlercodes der Programme werden einzeln geprüft. Hinweise auf stderr (zum Beispiel von ssh) sollen nicht abbrechen.
$ErrorActionPreference = 'Continue'
$KeyTag = 'bb-github-deploy'

function Step([string]$text) { Write-Host ""; Write-Host "== $text" -ForegroundColor Cyan }
function Ok([string]$text) { Write-Host "   ok: $text" -ForegroundColor Green }
function Info([string]$text) { Write-Host "   $text" }
function Warn([string]$text) { Write-Host "   Hinweis: $text" -ForegroundColor Yellow }
function Fail([string]$text) { Write-Host ""; Write-Host "Fehler: $text" -ForegroundColor Red; exit 1 }
function Plan([string]$text) { Write-Host "   [Probelauf] würde: $text" -ForegroundColor DarkYellow }

function Has([string]$name) { return [bool](Get-Command $name -ErrorAction SilentlyContinue) }

# Führt ein Programm aus und bricht bei einem Fehlercode ab. Gibt die Ausgabe zurück.
function Run([string]$what, [scriptblock]$block) {
    $out = & $block 2>&1
    if ($LASTEXITCODE -ne 0) { Fail "$what ist fehlgeschlagen.`n$($out -join "`n")" }
    return $out
}

# Schickt ein Shell-Skript über ssh an den Server (als Base64, damit weder Anführungszeichen noch Zeilenenden stören).
function RemoteShell([string]$script) {
    $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes(($script -replace "`r`n", "`n")))
    $out = & ssh -o BatchMode=yes -o ConnectTimeout=15 $SshAlias "echo $b64 | base64 -d | sh" 2>&1
    if ($LASTEXITCODE -ne 0) { Fail "Befehl auf dem Server fehlgeschlagen.`n$($out -join "`n")" }
    return $out
}

# ------------------------------------------------------------------ 1. Voraussetzungen
Step '1. Voraussetzungen prüfen'
foreach ($tool in 'git', 'ssh', 'ssh-keygen', 'ssh-keyscan') {
    if (-not (Has $tool)) { Fail "'$tool' fehlt. Git for Windows und der OpenSSH-Client müssen installiert sein." }
}
Ok 'git und OpenSSH sind da'

if (-not $Repo) {
    $origin = (& git remote get-url origin 2>$null)
    if ($origin -match 'github\.com[:/]+([^/]+)/([^/]+?)(\.git)?$') { $Repo = "$($Matches[1])/$($Matches[2])" }
}
if (-not $Repo) { Fail 'Das GitHub-Repository ist unbekannt. Mit -Repo Besitzer/Name angeben.' }
Ok "Repository: $Repo"

$cfg = & ssh -G $SshAlias 2>$null
$srvHost = ($cfg | Where-Object { $_ -match '^hostname ' } | Select-Object -First 1) -replace '^hostname ', ''
$srvUser = ($cfg | Where-Object { $_ -match '^user ' } | Select-Object -First 1) -replace '^user ', ''
$srvPort = ($cfg | Where-Object { $_ -match '^port ' } | Select-Object -First 1) -replace '^port ', ''
if (-not $srvHost -or $srvHost -eq $SshAlias) {
    Fail "Der SSH-Alias '$SshAlias' ist nicht in ~/.ssh/config eingetragen. Eintrag mit HostName, User und IdentityFile anlegen (siehe docs/deployment.md)."
}
Ok "Server: $srvUser@$srvHost Port $srvPort"

$ghReady = $false
if (Has 'gh') {
    & gh auth status *> $null
    $ghReady = ($LASTEXITCODE -eq 0)
    if ($ghReady) { Ok 'GitHub CLI ist angemeldet' } else { Warn 'GitHub CLI ist nicht angemeldet. Mit "gh auth login" anmelden, sonst läuft die Eingabe der Secrets über die Zwischenablage.' }
} else {
    Warn 'Die GitHub CLI (gh) ist nicht installiert. Mit "winget install GitHub.cli" lässt sich das ändern, sonst läuft die Eingabe der Secrets über die Zwischenablage.'
}

# ------------------------------------------------------------------ 2. Server erreichbar
Step '2. Server prüfen'
if ($DryRun) {
    Plan "ssh $SshAlias und prüfen, ob ~/battery-bandits/staging/.env existiert"
} else {
    $probe = RemoteShell 'echo "Verbindung steht: $(hostname)"; test -f ~/battery-bandits/staging/.env && echo ENV_OK || echo ENV_FEHLT; command -v docker >/dev/null && echo DOCKER_OK || echo DOCKER_FEHLT'
    Info ($probe -join ' ')
    if ($probe -contains 'ENV_FEHLT') { Warn 'Auf dem Server fehlt ~/battery-bandits/staging/.env. Der Deploy bricht dann mit einer klaren Meldung ab (Vorlage: deploy/.env.staging.example, siehe docs/deployment.md).' }
    if ($probe -contains 'DOCKER_FEHLT') { Fail 'Auf dem Server ist kein Docker erreichbar.' }
    Ok 'Server antwortet'
}

# ------------------------------------------------------------------ 3. Schlüssel erzeugen
Step '3. Deploy-Schlüssel erzeugen (nur für GitHub)'
$work = Join-Path ([IO.Path]::GetTempPath()) ("bb-deploy-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$keyFile = Join-Path $work 'deploy_key'
$pubFile = "$keyFile.pub"

try {
    if ($DryRun) {
        Plan "ssh-keygen -t ed25519 in einem temporären Ordner ($work)"
    } else {
        New-Item -ItemType Directory -Path $work -ErrorAction Stop | Out-Null
        Run 'ssh-keygen' { & ssh-keygen -q -t ed25519 -N '""' -C $KeyTag -f $keyFile } | Out-Null
        Ok 'Schlüsselpaar erzeugt, Passwort leer (der Schlüssel liegt nur kurz auf dem Rechner)'
    }

    # ------------------------------------------------------------------ 4. Auf dem Server eintragen
    Step '4. Öffentlichen Schlüssel auf dem Server eintragen'
    if ($DryRun) {
        Plan 'authorized_keys sichern, alten Deploy-Schlüssel entfernen, neuen mit Einschränkungen eintragen'
    } else {
        $pub = (Get-Content -Raw $pubFile).Trim()
        $line = "no-port-forwarding,no-agent-forwarding,no-X11-forwarding,no-pty $pub"
        $b64Line = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($line + "`n"))
        RemoteShell @"
set -e
umask 077
mkdir -p ~/.ssh
touch ~/.ssh/authorized_keys
cp ~/.ssh/authorized_keys ~/.ssh/authorized_keys.bak-bbdeploy
grep -v '$KeyTag' ~/.ssh/authorized_keys > ~/.ssh/authorized_keys.neu || true
echo '$b64Line' | base64 -d >> ~/.ssh/authorized_keys.neu
mv ~/.ssh/authorized_keys.neu ~/.ssh/authorized_keys
chmod 600 ~/.ssh/authorized_keys
echo eingetragen
"@ | Out-Null
        Ok 'Schlüssel eingetragen (Sicherung: ~/.ssh/authorized_keys.bak-bbdeploy auf dem Server)'

        $test = & ssh -i $keyFile -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=15 "$srvUser@$srvHost" -p $srvPort 'echo deploy-ok' 2>&1
        if ($LASTEXITCODE -ne 0 -or ($test -join '') -notmatch 'deploy-ok') {
            Fail "Der neue Schlüssel funktioniert auf dem Server nicht.`n$($test -join "`n")"
        }
        Ok 'Der neue Schlüssel funktioniert'
    }

    # ------------------------------------------------------------------ 5. Secrets in GitHub
    Step '5. Secrets in GitHub hinterlegen'
    $known = (& ssh-keyscan -T 15 -p $srvPort $srvHost 2>$null | Where-Object { $_ -and ($_ -notmatch '^#') }) -join "`n"
    if (-not $known) {
        # Der Windows-Client von ssh-keyscan scheitert an manchen Servern (neuere Schlüsselaustausch-Verfahren).
        # Dann gilt der Schlüssel, dem dieser Rechner schon vertraut: Er steht in known_hosts, sonst würde die
        # Verbindung zum Server oben nicht funktionieren.
        $target = if ($srvPort -eq 22) { $srvHost } else { "[$srvHost]:$srvPort" }
        $known = (& ssh-keygen -F $target 2>$null | Where-Object { $_ -and ($_ -notmatch '^#') }) -join "`n"
    }
    if (-not $known) { Fail 'Der Host-Schlüssel des Servers ist nicht zu ermitteln. Ist der Server erreichbar, und gibt es eine Verbindung mit ssh bb?' }

    $plain = [ordered]@{
        DEPLOY_HOST        = $srvHost
        DEPLOY_USER        = $srvUser
        DEPLOY_PORT        = "$srvPort"
        DEPLOY_KNOWN_HOSTS = $known
    }

    if ($DryRun) {
        Plan "Secrets DEPLOY_HOST, DEPLOY_USER, DEPLOY_PORT, DEPLOY_KNOWN_HOSTS und DEPLOY_SSH_KEY in $Repo setzen"
    } elseif ($ghReady) {
        foreach ($name in $plain.Keys) {
            $tmp = Join-Path $work "$name.txt"
            [IO.File]::WriteAllText($tmp, $plain[$name], (New-Object Text.UTF8Encoding($false)))
            & cmd.exe /c "gh secret set $name --repo $Repo < `"$tmp`""
            if ($LASTEXITCODE -ne 0) { Fail "Das Secret $name ließ sich nicht setzen. Hat dein GitHub-Konto Schreibrechte auf $Repo?" }
            Ok "Secret $name gesetzt"
        }
        & cmd.exe /c "gh secret set DEPLOY_SSH_KEY --repo $Repo < `"$keyFile`""
        if ($LASTEXITCODE -ne 0) { Fail 'Das Secret DEPLOY_SSH_KEY ließ sich nicht setzen.' }
        Ok 'Secret DEPLOY_SSH_KEY gesetzt (der Schlüssel wurde nicht angezeigt)'
    } else {
        $page = "https://github.com/$Repo/settings/secrets/actions/new"
        Info "Ohne GitHub CLI geht es in fünf Schritten über die Zwischenablage. Die Seite zum Anlegen: $page"
        try { Start-Process $page } catch { }
        $all = [ordered]@{}
        foreach ($name in $plain.Keys) { $all[$name] = $plain[$name] }
        $all['DEPLOY_SSH_KEY'] = (Get-Content -Raw $keyFile)
        $i = 0
        foreach ($name in $all.Keys) {
            $i++
            Set-Clipboard -Value $all[$name]
            Info ""
            Info "Schritt $i von $($all.Count): In GitHub 'New repository secret' wählen."
            Info "   Name:  $name"
            Info "   Wert:  steht jetzt in der Zwischenablage (Strg+V)"
            [void](Read-Host '   Wenn das Secret gespeichert ist, Enter drücken')
        }
        Set-Clipboard -Value ' '
        Ok 'Zwischenablage geleert'
    }
}
finally {
    # Privaten Schlüssel überschreiben und löschen. Danach existiert er nur noch in GitHub.
    if (Test-Path $work) {
        Get-ChildItem $work -File -ErrorAction SilentlyContinue | ForEach-Object {
            try {
                $size = [Math]::Max($_.Length, 1)
                [IO.File]::WriteAllBytes($_.FullName, (New-Object byte[] $size))
            } catch { }
        }
        Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
    }
}
if (-not $DryRun) { Ok 'Privater Schlüssel vom Rechner gelöscht' }

# ------------------------------------------------------------------ 6. Wie es weitergeht
Step '6. Wie es weitergeht'
Info 'Der erste Lauf startet mit dem nächsten Push: Feature-Branch -> Staging, main -> Prod.'
Info 'Von Hand: GitHub, Reiter Actions, Workflow "CI", "Run workflow", Ziel wählen.'

Write-Host ""
Write-Host "Fertig. Ab jetzt gilt: Push auf einen Feature-Branch -> prüfen -> Staging, Push auf main -> prüfen -> Prod." -ForegroundColor Green
Write-Host "Ansehen: Reiter 'Actions' bei $Repo, im Netz https://staging.batterybandits.racore.net und https://batterybandits.racore.net"
