# eafar/build.ps1 — one-command build + test.
# Uses the local winlibs toolchain (GCC 16 / C++23, CMake 4.3).
$ErrorActionPreference = "Stop"
$toolchain = "C:\Users\Lab\Documents\toolchains\winlibs\mingw64\bin"
$env:Path = "$toolchain;$env:Path"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$build = Join-Path $root "build"

cmake -S $root -B $build -G "MinGW Makefiles" -DCMAKE_BUILD_TYPE=Release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cmake --build $build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& (Join-Path $build "eafar_tests.exe")
exit $LASTEXITCODE
