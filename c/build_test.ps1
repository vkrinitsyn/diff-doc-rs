Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$MSVC = "C:\Program Files\Microsoft Visual Studio\18\Insiders\VC\Tools\MSVC\14.50.35717"
$SDK  = "C:\Program Files (x86)\Windows Kits\10"
$VER  = "10.0.19041.0"

$cl = Join-Path $MSVC "bin\Hostx64\x64\cl.exe"

$inc = @(
    "/I$MSVC\include",
    "/I$SDK\Include\$VER\ucrt",
    "/I$SDK\Include\$VER\shared",
    "/I$SDK\Include\$VER\um"
)

$lib = @(
    "/LIBPATH:$MSVC\lib\x64",
    "/LIBPATH:$SDK\Lib\$VER\ucrt\x64",
    "/LIBPATH:$SDK\Lib\$VER\um\x64"
)

$srcs = @("diff_apply.c", "mismatches.pb-c.c", "test_diff_apply.c")

Write-Host "=== Compiling ==="
& $cl @inc /W3 /WX- /D_CRT_SECURE_NO_WARNINGS @srcs /Fetest_diff_apply.exe /link @lib
if ($LASTEXITCODE -ne 0) { Write-Error "Compilation failed"; exit 1 }

Write-Host ""
Write-Host "=== Running tests ==="
& ".\test_diff_apply.exe"
exit $LASTEXITCODE
