$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$dxc = New-Item -Path (Join-Path $env:RUNNER_TEMP 'tc-dxc-gate') -ItemType Directory -Force
$archive = Join-Path $dxc.FullName 'compiler.zip'
Invoke-WebRequest -Uri 'https://api.nuget.org/v3-flatcontainer/microsoft.direct3d.dxc/1.9.2609.5/microsoft.direct3d.dxc.1.9.2609.5.nupkg' -OutFile $archive -TimeoutSec 60
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne '73cfa082245918c2cc1ec6740250eb7377e83a757b3b3a3e16e0d71535bfbfc3') {
    throw 'DXC package hash did not match'
}
Expand-Archive -LiteralPath $archive -DestinationPath $dxc.FullName
$dxcBin = Join-Path $dxc.FullName 'build/native/bin/x64'
$dxcLibrary = Join-Path $dxcBin 'dxcompiler.dll'
if (!(Test-Path -LiteralPath $dxcLibrary)) { throw 'DXC library missing' }
$env:PATH = "$dxcBin;$env:PATH"
$env:WGPU_DX12_COMPILER = 'dxc'
Write-Host "Gate using pinned DXC $((Get-Item -LiteralPath $dxcLibrary).VersionInfo.ProductVersion)"
cargo xtask ci
exit $LASTEXITCODE
