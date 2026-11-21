@echo off
setlocal
where cargo >nul 2>nul || (echo Rust is missing. Install Rust from https://rustup.rs/ & pause & exit /b 1)
where node >nul 2>nul || (echo Node.js is missing. Install Node.js 20+ or 22 LTS. & pause & exit /b 1)
where ffmpeg >nul 2>nul || (echo FFmpeg is missing from PATH. Run install-ffmpeg.ps1 first. & pause & exit /b 1)
where ffprobe >nul 2>nul || (echo FFprobe is missing from PATH. Install the full FFmpeg package. & pause & exit /b 1)

REM Keep Cargo build output outside the Vite project to avoid watcher collisions.
if not exist "%LOCALAPPDATA%\CropStudio\cargo-target" mkdir "%LOCALAPPDATA%\CropStudio\cargo-target"
set "CARGO_TARGET_DIR=%LOCALAPPDATA%\CropStudio\cargo-target"

if not exist node_modules call npm install
if errorlevel 1 (echo npm install failed. & pause & exit /b 1)
npm run tauri dev
if errorlevel 1 (echo CropStudio development run failed. & pause & exit /b 1)
pause
