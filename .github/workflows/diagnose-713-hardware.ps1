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
set "TC_GPU_PROBE_SUBMISSIONS=combined"
if /i "%~1"=="split" set "TC_GPU_PROBE_SUBMISSIONS=split"
"%~dp0texturecraft-render-tests.exe" gpu_composite::procedural_tests::authored_material_images_match_cpu_projections_and_decal_coverage --exact --test-threads=1 --nocapture > "%~dp0texturecraft-713-dx12-%TC_GPU_PROBE_SUBMISSIONS%.log" 2>&1
set "TC_TEST_RESULT=%errorlevel%"
type "%~dp0texturecraft-713-dx12-%TC_GPU_PROBE_SUBMISSIONS%.log"
echo.
echo Exit code: %TC_TEST_RESULT%
echo Log: %~dp0texturecraft-713-dx12-%TC_GPU_PROBE_SUBMISSIONS%.log
pause
exit /b %TC_TEST_RESULT%
'@ | Set-Content -Path (Join-Path $output.FullName 'Run-TextureCraft-Direct3D-test.cmd')

# The shared gate packages the executable. Diagnostic outcomes are kept in
# their individual logs, including native process exits.
$case = 'gpu_composite::procedural_tests::authored_material_images_match_cpu_projections_and_decal_coverage'
function Invoke-WarpProbe($program, $compiler, $submissions) {
    $env:WGPU_BACKEND = 'dx12'
    $env:WGPU_DX12_COMPILER = $compiler
    $env:TC_GPU_PROBE_SUBMISSIONS = $submissions
    $log = Join-Path $output.FullName "warp-$compiler-$submissions.log"
    $errors = "$log.stderr"
    $test = Start-Process -FilePath $program -ArgumentList @($case, '--exact', '--test-threads=1', '--nocapture') `
        -RedirectStandardOutput $log -RedirectStandardError $errors -NoNewWindow -PassThru
    if ($test.WaitForExit(240000)) {
        $result = $test.ExitCode
    } else {
        $test.Kill()
        $test.WaitForExit()
        $result = 'timed out after 240 seconds'
    }
    Add-Content -LiteralPath $log -Value (Get-Content -LiteralPath $errors)
    Remove-Item -LiteralPath $errors
    Add-Content -LiteralPath $log -Value "WARP $compiler $submissions result $result"
    Get-Content -LiteralPath $log | Write-Host
}

$dxc = New-Item -Path (Join-Path $env:RUNNER_TEMP 'tc-dxc') -ItemType Directory -Force
$archive = Join-Path $dxc.FullName 'compiler.zip'
try {
    Invoke-WebRequest -Uri 'https://api.nuget.org/v3-flatcontainer/microsoft.direct3d.dxc/1.9.2609.5/microsoft.direct3d.dxc.1.9.2609.5.nupkg' -OutFile $archive -TimeoutSec 60
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne '73cfa082245918c2cc1ec6740250eb7377e83a757b3b3a3e16e0d71535bfbfc3') {
        throw 'DXC package hash did not match'
    }
    Expand-Archive -LiteralPath $archive -DestinationPath $dxc.FullName
    $dxcBin = Join-Path $dxc.FullName 'build/native/bin/x64'
    $dxcExe = Join-Path $dxcBin 'texturecraft-render-tests.exe'
    Copy-Item -LiteralPath $exe -Destination $dxcExe
    Invoke-WarpProbe $dxcExe 'dxc' 'combined'
} catch {
    Write-Host "DXC setup or probe failed: $_"
}
Invoke-WarpProbe $exe 'fxc' 'split'
