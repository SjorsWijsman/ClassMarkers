@echo off
:: Brings Class Markers up to date after a Convergence update: re-reads Convergence's files,
:: rebuilds the plugin and puts it back in the launch profiles (the plugin remakes its icons in
:: game). Needs Python and Rust (see README.md).
echo Updating Class Markers for the installed Convergence...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\update.ps1"
if errorlevel 1 (
    echo Update failed - see the message above.
) else (
    echo.
    echo Done. Start the game with Start_Convergence.bat as usual.
)
pause
