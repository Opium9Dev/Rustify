@echo off
setlocal
for %%I in ("%~dp0..") do set "PROJECT_ROOT=%%~fI"
cd /d "%PROJECT_ROOT%"
if errorlevel 1 (
    echo Failed to switch to the project directory. 1>&2
    exit /b 1
)

where cargo >nul 2>nul
if errorlevel 1 (
    echo Cargo was not found. Install Rust and ensure cargo is on PATH. 1>&2
    exit /b 1
)

if not exist "assets\app.ico" (
    echo Missing icon: assets\app.ico 1>&2
    exit /b 1
)

set "CARGO_ENCODED_RUSTFLAGS=--remap-path-prefix=%CD%=."

echo Building optimized Windows release...
cargo build --release --locked
if errorlevel 1 exit /b %errorlevel%

if not exist "target\release\rustify.exe" (
    echo Build completed but target\release\rustify.exe was not found. 1>&2
    exit /b 1
)

if not exist "dist" mkdir "dist"
if errorlevel 1 (
    echo Failed to create the dist directory. 1>&2
    exit /b 1
)

copy /y "target\release\rustify.exe" "dist\rustify.exe" >nul
if errorlevel 1 (
    echo Failed to copy the release executable to dist. 1>&2
    exit /b 1
)

echo Published single-file executable: dist\rustify.exe
endlocal
