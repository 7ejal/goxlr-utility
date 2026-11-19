@echo off
rem User-controlled 909 update and reversible native test. Adjust these paths for a later lab session.
set "PROJECT=C:\Users\real5\Documents\ChatGPT\Goxlr\native-mic-eq-worktree"
set "LAB=C:\Users\real5\Documents\codex\2026-11-19\referenced-chatgpt-conversation-this-is-an\work"
set "BACKUP=%LAB%\pre-909-approved-backup"
set "TEST_RUNNER=%PROJECT%\target\debug\deps\goxlr_native_mic_eq-49dea9ef42a187c1.exe"
set "READER=C:\Users\real5\Documents\codex\2026-11-18\referenced-chatgpt-conversation-this-is-an-10\work\clean-windows-build\release\examples\read_goxlr_recovery.exe"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%PROJECT%\tools\Test-GoXLR909Hardware.ps1" -BackupDirectory "%BACKUP%" -OutputDirectory "%LAB%\physical-909-user-test-%RANDOM%" -NativeTestRunner "%TEST_RUNNER%" -RecoveryReader "%READER%"
if errorlevel 1 echo Test stopped. Keep the console log and do not repeat the update.
pause
