$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$artifacts = cargo test --locked -p texturecraft-render --lib --no-run --message-format=json |
    ForEach-Object { ConvertFrom-Json -InputObject $_ -ErrorAction SilentlyContinue } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'texturecraft_render' -and $_.profile.test -and $_.executable }
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$exe = ($artifacts | Select-Object -Last 1).executable
if (!$exe) { throw 'Renderer test executable missing' }
$output = New-Item -Path 'diagnostic-artifact' -ItemType Directory -Force
Copy-Item -LiteralPath $exe -Destination (Join-Path $output.FullName 'texturecraft-render-tests.exe')
@'
@echo off
setlocal
set "WGPU_BACKEND=dx12"
"%~dp0texturecraft-render-tests.exe" gpu_composite::procedural_tests::authored_material_images_match_cpu_projections_and_decal_coverage --exact --test-threads=1 --nocapture > "%~dp0texturecraft-713-dx12.log" 2>&1
set "TC_TEST_RESULT=%errorlevel%"
type "%~dp0texturecraft-713-dx12.log"
echo.
echo Exit code: %TC_TEST_RESULT%
echo Log: %~dp0texturecraft-713-dx12.log
pause
exit /b %TC_TEST_RESULT%
'@ | Set-Content -Path (Join-Path $output.FullName 'Run-TextureCraft-Direct3D-test.cmd')
