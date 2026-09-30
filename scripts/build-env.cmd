@echo off
setlocal
call "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" arm64 >nul 2>&1
if errorlevel 1 (
  echo VCVARSALL FAILED
  exit /b 1
)
set "CARGO_TARGET_DIR=C:\vpd\src-tauri\target\windows-arm64"
set "PATH=C:\Program Files\LLVM\bin;C:\Program Files\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja;C:\Users\antoin\node-install\node-v24.19.0-win-arm64;C:\Users\antoin\.cargo\bin;C:\Users\antoin\AppData\Roaming\npm;C:\Users\antoin\python312;C:\Users\antoin\git-portable\cmd;%PATH%"
REM Use clang-cl and Ninja for native ARM64 dependencies.
set "CMAKE_GENERATOR=Ninja"
set "CMAKE_C_COMPILER=C:/Program Files/LLVM/bin/clang-cl.exe"
set "CMAKE_CXX_COMPILER=C:/Program Files/LLVM/bin/clang-cl.exe"
REM E1: clang is a requirement anyway, and CMake must be told which compiler.
REM E2: bindgen cannot resolve whisper.h under clang for aarch64-pc-windows-msvc
REM (libclang produces opaque structs). Use the bindings shipped with whisper-rs-sys,
REM patched for MSVC enum semantics (gretype/sampling_strategy/aheads = c_int).
set "WHISPER_DONT_GENERATE_BINDINGS=1"
cargo %*
exit /b %errorlevel%
