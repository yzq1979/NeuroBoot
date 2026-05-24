@echo off
REM 02-run-copype.cmd
REM Stage 6.3: Initialize WinPE workspace via ADK copype amd64.
REM
REM copype.cmd needs %WinPERoot% / %OSCDImgRoot% env vars which DandISetEnv.bat sets.
REM This wrapper sources DandISetEnv.bat first, then calls copype.

set ADK_ROOT=C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit
set WORKSPACE=C:\NeuroBoot\pe-build\workspace

echo === Sourcing DandISetEnv.bat ===
call "%ADK_ROOT%\Deployment Tools\DandISetEnv.bat"
echo.
echo WinPERoot=%WinPERoot%
echo OSCDImgRoot=%OSCDImgRoot%
echo.

echo === Running copype amd64 %WORKSPACE% ===
"%ADK_ROOT%\Windows Preinstallation Environment\copype.cmd" amd64 "%WORKSPACE%"
exit /b %errorlevel%
