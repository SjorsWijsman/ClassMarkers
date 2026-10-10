@echo off
:: Removes Class Markers from The Convergence's launch profiles.
echo Removing Class Markers from The Convergence's launch profiles...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\profiles.ps1" uninstall
if errorlevel 1 (
    echo Uninstall failed - see the message above.
) else (
    echo Done. The Convergence will start without Class Markers.
)
pause
