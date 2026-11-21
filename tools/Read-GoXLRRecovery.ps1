# Runs recovery reads, an experimental update, or validation with profiles preserved.
param(
    [Parameter(Mandatory)][string]$ResearchDaemon,
    [Parameter(Mandatory)][string]$RecoveryReader,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Firmware,
    [string]$AuthorizedSha256,
    [string]$ValidationScript,
    [hashtable]$ValidationArguments = @{},
    [string]$OriginalDaemon = 'C:\Program Files\GoXLR Utility\goxlr-daemon.exe',
    [string]$Endpoint = 'http://127.0.0.1:14564/api/command',
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'

function Request($body) {
    Invoke-RestMethod -Uri $Endpoint -Method Post -ContentType 'application/json' -Body (
        ConvertTo-Json -InputObject $body -Depth 30 -Compress
    ) -TimeoutSec 10
}
function Wait-Status {
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        try {
            $reply = Request 'GetStatus'
            if ($reply.Status -and $reply.Status.mixers.PSObject.Properties.Count -gt 0) { return $reply.Status }
        } catch { Start-Sleep -Milliseconds 200 }
        Start-Sleep -Milliseconds 100
    }
    throw 'The GoXLR daemon did not become ready.'
}

$OriginalDaemon = (Resolve-Path -LiteralPath $OriginalDaemon).Path
$ResearchDaemon = (Resolve-Path -LiteralPath $ResearchDaemon).Path
$RecoveryReader = (Resolve-Path -LiteralPath $RecoveryReader).Path
$running = @(Get-Process -Name goxlr-daemon | Where-Object Path -eq $OriginalDaemon)
if ($running.Count -ne 1) { throw 'Expected exactly one original daemon.' }
$status = Wait-Status
$mixers = @($status.mixers.PSObject.Properties)
if ($mixers.Count -ne 1) { throw 'Expected exactly one GoXLR.' }
$serial = $mixers[0].Name
$mixer = $mixers[0].Value
if ($mixer.hardware.device_type -ne 'Full') { throw 'Expected a full-size GoXLR.' }
if ($Firmware -and $ValidationScript) { throw 'Choose firmware updating or validation, not both.' }
if ($ValidationScript) { $ValidationScript = (Resolve-Path -LiteralPath $ValidationScript).Path }
if ($DryRun) {
    'Would preserve profiles, temporarily switch the daemon, run the selected operation, and restore the original daemon.'
    return
}
if ($Firmware) {
    $Firmware = (Resolve-Path -LiteralPath $Firmware).Path
    $image = [IO.File]::ReadAllBytes($Firmware)
    $build = [BitConverter]::ToUInt32($image, 0x1c)
    $approvedImages = @{
        908 = 'C8607989063552E5A412AD3B29B4E1DB355D9B737DCC59D972A5349FBFAA970F'
        909 = '74555B3667DCB7150930AE544BDFCF732FEBF7BB6A0AA9B7CFD9B513FFA3C74B'
        910 = 'B524AC6CF1B937E9C5D224CE9794E807E3B6561652B94DC6B26111FC9EF49744'
    }
    if ($image.Length -ne 640044 -or [Text.Encoding]::ASCII.GetString($image, 0, 14) -ne 'GoXLR Firmware' -or
        !$approvedImages.ContainsKey([int]$build) -or [BitConverter]::ToUInt32($image, 0x28) -ne 0xe897 -or
        (Get-FileHash -LiteralPath $Firmware).Hash -ne $approvedImages[[int]$build]) {
        throw 'Expected an exact recorded same-size image.'
    }
    if ($build -eq 910) {
        $manifest = Get-Content -Encoding UTF8 -Raw (Join-Path $PSScriptRoot '../native-mic-eq/BUILD-910.json') | ConvertFrom-Json
        if (!$manifest.flash_authorized -or $AuthorizedSha256 -ne $approvedImages[910] -or
            $manifest.sha256 -ne $AuthorizedSha256 -or
            (Get-FileHash $OriginalDaemon).Hash -ne $manifest.daemon_sha256 -or
            (Get-FileHash $ResearchDaemon).Hash -ne $manifest.daemon_sha256 -or
            ($mixer.hardware.versions.firmware -join '.') -ne '1.5.1.909') {
            throw '910 requires its exact authorization, matching daemon and running 909 baseline; never retry an update automatically.'
        }
    }
    if ($build -eq 909) {
        $manifest = Get-Content -Encoding UTF8 -Raw (Join-Path $PSScriptRoot '../native-mic-eq/BUILD-909.json') | ConvertFrom-Json
        if ((Get-FileHash $OriginalDaemon).Hash -ne $manifest.windows_daemon_sha256 -or
            (Get-FileHash $ResearchDaemon).Hash -ne $manifest.windows_daemon_sha256) {
            throw 'Install the recorded 909-compatible daemon before this update; the older daemon cannot preserve native EQ replay on 909.'
        }
    }
}
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Choose a new output directory.' }
$output = (New-Item -ItemType Directory -Path $OutputDirectory).FullName
$files = @(
    @{ Directory = $status.paths.profile_directory; Name = $mixer.profile_name + '.goxlr'; Backup = 'original-mixer.goxlr' },
    @{ Directory = $status.paths.mic_profile_directory; Name = $mixer.mic_profile_name + '.goxlrMicProfile'; Backup = 'original-mic.goxlrMicProfile' }
)
foreach ($file in $files) {
    if ([IO.Path]::GetFileName($file.Name) -ne $file.Name) { throw 'Invalid profile filename.' }
    $file.Source = (Resolve-Path -LiteralPath (Join-Path $file.Directory $file.Name)).Path
    $file.Copy = Join-Path $output $file.Backup
    Copy-Item -LiteralPath $file.Source -Destination $file.Copy
    $file.Hash = (Get-FileHash -LiteralPath $file.Copy).Hash
}
$status | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $output 'status-before.json')
$files | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'profile-backups.json')
$switched = $false
$research = $null
try {
    foreach ($command in 'SaveProfile', 'SaveMicProfile') {
        $operation = @{}
        $operation[$command] = @()
        $reply = Request @{ Command = @($serial, $operation) }
        if ($reply -ne 'Ok') { throw "Could not save live settings. $reply" }
    }
    $null = Request @{ Daemon = 'StopDaemon' }
    $switched = $true
    if (!$running[0].WaitForExit(15000)) { throw 'The original daemon did not exit.' }
    $research = Start-Process -FilePath $ResearchDaemon -ArgumentList @('--disable-tray', 'true') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $output 'research-daemon.log') -RedirectStandardError (Join-Path $output 'research-daemon.err')
    $null = Wait-Status
    if ($ValidationScript) {
        & $ValidationScript -OutputDirectory $output @ValidationArguments
    } elseif ($Firmware) {
        $reply = Request @{ RunFirmwareUpdate = @($serial, $Firmware, $false) }
        if ($reply -ne 'Ok') { throw "Firmware update request failed. $reply" }
        $last = ''
        $complete = $false
        for ($attempt = 0; $attempt -lt 600; $attempt++) {
            $update = (Request 'GetStatus').Status.firmware.$serial
            if ($update.error) { throw $update.error }
            $phase = $update.state | ConvertTo-Json -Compress -Depth 5
            if ($phase -ne $last) { "Firmware phase $phase"; $last = $phase }
            if ($phase -eq '"Failed"') { throw 'Firmware update failed.' }
            if ($phase -eq '"Complete"') { $complete = $true; break }
            Start-Sleep -Milliseconds 500
        }
        if (!$complete) { throw 'Firmware update did not complete within five minutes.' }
        $booted = $false
        for ($attempt = 0; $attempt -lt 150; $attempt++) {
            try {
                $live = (Request 'GetStatus').Status
                if (($live.mixers.$serial.hardware.versions.firmware -join '.') -eq "1.5.1.$build") {
                    $booted = $true
                    break
                }
            } catch {}
            Start-Sleep -Milliseconds 200
        }
        if (!$booted) {
            throw "The device did not report experimental firmware $build after reconnect."
        }
        "Experimental firmware $build enumerated."
    } else {
      foreach ($partition in 'boot2', 'golden') {
        & $RecoveryReader $partition (Join-Path $output "$partition.bin")
        if ($LASTEXITCODE) { throw "Reading $partition failed." }
      }
    }
} finally {
    try {
      if ($switched) {
        if ($research -and (Get-Process -Id $research.Id -ErrorAction SilentlyContinue)) {
            try { $null = Request @{ Daemon = 'StopDaemon' } } catch {}
            if (Get-Process -Id $research.Id -ErrorAction SilentlyContinue) { Wait-Process -Id $research.Id -Timeout 15 }
        }
        Start-Process -FilePath $OriginalDaemon -WindowStyle Hidden | Out-Null
        $restored = Wait-Status
        foreach ($field in 'frequency', 'gain') {
          foreach ($item in $mixer.mic_status.equaliser.$field.PSObject.Properties) {
            if ($restored.mixers.$serial.mic_status.equaliser.$field.($item.Name) -ne $item.Value) {
                throw 'Original daemon restarted but live EQ differs. Profile backups are in the output directory.'
            }
          }
        }
      }
    } finally {
      foreach ($file in $files) {
        if ((Get-FileHash -LiteralPath $file.Copy).Hash -ne $file.Hash) { throw 'Profile backup hash changed.' }
        Copy-Item -LiteralPath $file.Copy -Destination $file.Source -Force
        if ((Get-FileHash -LiteralPath $file.Source).Hash -ne $file.Hash) { throw 'Saved profile restoration failed.' }
      }
    }
}
'Original daemon and saved profiles restored.'
