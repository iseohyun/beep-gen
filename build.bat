@echo off
echo ============================================================
echo Beep Audio Generator - PyInstaller EXE Build Script
echo ============================================================
echo.

echo [*] Closing any running BeepGen instances...
taskkill /F /IM BeepGen.exe >nul 2>&1
timeout /t 1 /nobreak >nul

echo [1/2] Checking dependencies...
python -m pip install -r requirements.txt pyinstaller

echo.
echo [2/2] Building standalone executable (BeepGen.exe)...
pyinstaller --noconfirm --clean --onefile --windowed --name BeepGen --add-data "generator.py;." --add-data "gui.py;." --add-data "tts_engine.py;." main.py

if %ERRORLEVEL% EQU 0 (
    echo.
    echo ============================================================
    echo [SUCCESS] Build completed successfully!
    echo Output binary: dist\BeepGen.exe
    echo ============================================================
) else (
    echo.
    echo [ERROR] Build failed with exit code %ERRORLEVEL%
)
