@echo off
:: Adds Class Markers to The Convergence's launch profiles, so Start_Convergence.bat loads it.
:: Run this again after updating The Convergence (an update replaces those profiles).
echo Installing Class Markers into The Convergence's launch profiles...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\profiles.ps1" install
if errorlevel 1 (
    echo Install failed - see the message above.
) else (
    echo Done. Start the game with Start_Convergence.bat as usual.
)
pause
