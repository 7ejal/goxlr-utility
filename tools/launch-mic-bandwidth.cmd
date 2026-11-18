@echo off
setlocal
rem Launch the portable build beside this script, or from the source tree's build.
set "APP=%~dp0"
if not exist "%APP%goxlr-daemon.exe" set "APP=%~dp0..\target\portable\release\"
set "PORT=14564"
if not exist "%APP%goxlr-daemon.exe" (
  echo The portable GoXLR daemon is missing.
  exit /b 1
)
if not exist "%APP%goxlr-utility-ui.exe" (
  echo The portable GoXLR desktop interface is missing.
  exit /b 1
)
if /i "%~1"=="--check" exit /b 0
tasklist /FI "IMAGENAME eq goxlr-daemon.exe" /NH | find /i "goxlr-daemon.exe" >nul
if not errorlevel 1 (
  echo Exit the existing GoXLR Utility from its system tray before starting the portable build.
  exit /b 2
)
start "GoXLR Utility daemon" /D "%APP%." "%APP%goxlr-daemon.exe" --http-bind-address 127.0.0.1 --http-port %PORT%
powershell -NoProfile -Command "$deadline=(Get-Date).AddSeconds(25); do { try { $r=Invoke-WebRequest -UseBasicParsing 'http://127.0.0.1:%PORT%/mic-bandwidth/index.html' -TimeoutSec 2; if ($r.Content -match 'Microphone EQ bandwidth') { exit 0 } } catch {} ; Start-Sleep -Milliseconds 400 } while ((Get-Date) -lt $deadline); exit 1"
if errorlevel 1 (
  echo The new daemon's microphone controls did not become available. Check its console.
  exit /b 3
)
start "" "%APP%goxlr-utility-ui.exe"
