@echo off
setlocal

set MSVC=C:\Program Files\Microsoft Visual Studio\18\Insiders\VC\Tools\MSVC\14.50.35717
set SDK=C:\Program Files (x86)\Windows Kits\10
set SDK_VER=10.0.19041.0

set INCLUDE=%MSVC%\include;%SDK%\Include\%SDK_VER%\ucrt;%SDK%\Include\%SDK_VER%\shared;%SDK%\Include\%SDK_VER%\um
set LIB=%MSVC%\lib\x64;%SDK%\Lib\%SDK_VER%\ucrt\x64;%SDK%\Lib\%SDK_VER%\um\x64

"%MSVC%\bin\Hostx64\x64\cl.exe" ^
    /W3 /WX- /std:c11 /D_CRT_SECURE_NO_WARNINGS ^
    diff_apply.c mismatches.pb-c.c test_diff_apply.c ^
    /Fetest_diff_apply.exe ^
    /link /SUBSYSTEM:CONSOLE

if errorlevel 1 (
    echo Compilation failed.
    exit /b 1
)

echo.
echo === Running tests ===
test_diff_apply.exe
exit /b %ERRORLEVEL%
