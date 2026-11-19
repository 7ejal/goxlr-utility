# Installs the validated daemon/app with executable backups and rollback on failure.
param(
    [Parameter(Mandatory)][string]$Daemon,
    [Parameter(Mandatory)][string]$Application,
    [Parameter(Mandatory)][string]$ApplicationDirectory,
    [Parameter(Mandatory)][string]$BackupDirectory,
    [string]$InstalledDaemon = 'C:/Program Files/GoXLR Utility/goxlr-daemon.exe',
    [string]$Endpoint = 'http://127.0.0.1:14564/api/command',
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
$Daemon = (Resolve-Path -LiteralPath $Daemon).Path
$Application = (Resolve-Path -LiteralPath $Application).Path
$InstalledDaemon = (Resolve-Path -LiteralPath $InstalledDaemon).Path
$destination = Join-Path (Resolve-Path -LiteralPath $ApplicationDirectory).Path 'GoXLR-Native-Mic-EQ.exe'
if (!(Test-Path -LiteralPath $destination)) { throw 'The existing native app was not found.' }
$running = @(Get-Process goxlr-daemon | Where-Object Path -eq $InstalledDaemon)
if ($running.Count -ne 1 -or @(Get-Process goxlr-daemon).Count -ne 1) { throw 'Expected only the original daemon.' }
if ($DryRun) { 'Would back up both executables, close the native app, replace them, and restart the daemon/app.'; return }
if (Test-Path -LiteralPath $BackupDirectory) { throw 'Choose a new backup directory.' }
$backup = (New-Item -ItemType Directory -Path $BackupDirectory).FullName
$originalDaemon = Join-Path $backup 'goxlr-daemon.exe'
$originalApp = Join-Path $backup 'GoXLR-Native-Mic-EQ.exe'
Copy-Item -LiteralPath $InstalledDaemon -Destination $originalDaemon
Copy-Item -LiteralPath $destination -Destination $originalApp
foreach ($app in @(Get-Process GoXLR-Native-Mic-EQ -ErrorAction SilentlyContinue | Where-Object Path -eq $destination)) {
    if (!$app.CloseMainWindow()) { throw 'Close the existing native app before installing.' }
    Wait-Process -Id $app.Id -Timeout 15
}
$stopped = $false
$installed = $false
try {
    $reply = Invoke-RestMethod $Endpoint -Method Post -ContentType application/json -Body '{"Daemon":"StopDaemon"}'
    if ($reply -ne 'Ok') { throw 'Could not stop the original daemon.' }
    Wait-Process -Id $running[0].Id -Timeout 15
    $stopped = $true
    Copy-Item -LiteralPath $Daemon -Destination $InstalledDaemon -Force
    Copy-Item -LiteralPath $Application -Destination $destination -Force
    if ((Get-FileHash $InstalledDaemon).Hash -ne (Get-FileHash $Daemon).Hash -or
        (Get-FileHash $destination).Hash -ne (Get-FileHash $Application).Hash) { throw 'Installed executable hash mismatch.' }
    $installed = $true
} finally {
    if ($stopped) {
        try {
            if (!$installed) {
                if ((Get-FileHash $InstalledDaemon).Hash -ne (Get-FileHash $originalDaemon).Hash) {
                    Copy-Item -LiteralPath $originalDaemon -Destination $InstalledDaemon -Force
                }
                if ((Get-FileHash $destination).Hash -ne (Get-FileHash $originalApp).Hash) {
                    Copy-Item -LiteralPath $originalApp -Destination $destination -Force
                }
            }
        } finally {
            Start-Process -FilePath $InstalledDaemon -WindowStyle Hidden | Out-Null
            Start-Process -FilePath $destination | Out-Null
        }
    }
}
'Installed the daemon and existing native app; original executables are backed up.'
