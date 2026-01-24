@echo off
REM Cowork Build Script for Windows (local testing)
REM Builds both frontend and backend in one shot
REM For release builds with signing, use GitHub Actions

echo ========================================
echo Building Cowork
echo ========================================

REM Change to project root directory
cd /d "%~dp0"

echo.
echo [1/2] Building frontend...
echo ----------------------------------------
cd frontend
call npm ci
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: npm ci failed
    exit /b %ERRORLEVEL%
)

call npm run build
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Frontend build failed
    exit /b %ERRORLEVEL%
)
cd ..

echo.
echo [2/2] Building Tauri app...
echo ----------------------------------------
cd crates\cowork-app
call cargo tauri build --no-bundle
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Tauri build failed
    exit /b %ERRORLEVEL%
)
cd ..\..

echo.
echo ========================================
echo Build complete!
echo ========================================
echo.
echo Output location:
echo   target\release\cowork.exe
echo.
