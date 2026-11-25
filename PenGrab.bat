@echo off
setlocal
if /I "%~1"=="-Mode" goto powershell
if /I "%~1"=="-Language" goto powershell
if /I "%~1"=="-NoColor" goto powershell

if exist "%~dp0PenGrab.exe" (
    "%~dp0PenGrab.exe" %*
    goto done
)
if exist "%~dp0target\release\pengrab.exe" (
    "%~dp0target\release\pengrab.exe" %*
    goto done
)
if exist "%~dp0target\debug\pengrab.exe" (
    "%~dp0target\debug\pengrab.exe" %*
    goto done
)

echo PenGrab.exe was not found. Build locally with cargo build --release --locked.
exit /b 1

:powershell
set "PENGRAB_POWERSHELL=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
"%PENGRAB_POWERSHELL%" -NoProfile -File "%~dp0PenGrab.ps1" %*

:done
exit /b %ERRORLEVEL%
