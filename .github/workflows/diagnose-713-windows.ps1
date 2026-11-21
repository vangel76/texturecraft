$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$case = 'gpu_composite::procedural_tests::authored_material_images_match_cpu_projections_and_decal_coverage'
$artifacts = cargo test --locked -p texturecraft-render --lib --no-run --message-format=json |
    ForEach-Object { ConvertFrom-Json -InputObject $_ -ErrorAction SilentlyContinue } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'texturecraft_render' -and $_.profile.test -and $_.executable }
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$exe = ($artifacts | Select-Object -Last 1).executable
if (!$exe) { throw 'Renderer test executable missing' }
$start = Get-Date
& $exe $case --exact --test-threads=1 --nocapture
$baseline = $LASTEXITCODE

if ($baseline -ne 0) {
    Write-Host "Default shader flags exited with $baseline"
    $event = Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ID = 1000; StartTime = $start } -ErrorAction SilentlyContinue |
        Where-Object { $_.Message -like '*texturecraft_render*' } | Select-Object -First 1
    if ($event) { Write-Host $event.Message }

}
exit $baseline
