@echo off
REM Cowork Development Script for Windows
REM Runs frontend dev server and Tauri in dev mode

echo ========================================
echo Starting Cowork in development mode
echo ========================================

REM Change to project root directory
cd /d "%~dp0"

echo.
echo Installing frontend dependencies...
cd frontend
call npm install
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: npm install failed
    exit /b %ERRORLEVEL%
)
cd ..

echo.
echo Starting Tauri dev mode...
echo (This will start both frontend dev server and Tauri)
echo ----------------------------------------
cd crates\cowork-app
call cargo tauri dev
cd ..\..
