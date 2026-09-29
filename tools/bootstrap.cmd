@echo off
rem Runs bootstrap.ps1 regardless of the machine's PowerShell execution policy.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0bootstrap.ps1" %*
exit /b %ERRORLEVEL%
