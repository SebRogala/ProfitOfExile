@echo off
rem Release build of the synced desktop copy, without the installer (POE-249, 2026-09-07).
rem Run from anywhere: it works in its own parent directory (the dev copy root).
rem Close a running ProfitOfExile.exe first - Windows will not overwrite a running binary
rem and cargo then fails at the link step with "failed to remove file".
rem From WSL: make desktop-release-windows (syncs first). The exe lands at
rem src-tauri\target\release\ProfitOfExile.exe with the frontend embedded.
set "PATH=%USERPROFILE%\.cargo\bin;C:\Program Files\nodejs;%PATH%"
cd /d "%~dp0.."
echo build started %date% %time%
call npx.cmd tauri build --no-bundle
echo build exit %errorlevel% %date% %time%
