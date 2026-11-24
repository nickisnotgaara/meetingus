# Build/dev entrypoint for meetingus (sets MSVC + libclang + optional CUDA env).
#   .\build.ps1 test [cuda]        run cargo tests (CPU; with "cuda" -> GPU feature)
#   .\build.ps1 check [cuda]       cargo check
#   .\build.ps1 dev [cuda]         npm run tauri dev (hot reload)
#   .\build.ps1 release            npm run tauri build -- --features cuda (GPU release)
$ErrorActionPreference = "Stop"
$useCuda = $args -contains "cuda"

$vs = "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
if (Test-Path $vs) {
    cmd.exe /c "`"$vs`" >nul && set" | ForEach-Object {
        if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "Env:$($matches[1])" -Value $matches[2] }
    }
}
$python = Join-Path $env:LOCALAPPDATA "Programs\Python\Python314"
$env:LIBCLANG_PATH = Join-Path $python "Lib\site-packages\clang\native"
$cuda = "C:\cuda-env\Library"
# Python Scripts (ninja) + CUDA bin (nvcc) must precede MSVC for CUDA builds.
$env:PATH = "$python\Scripts;$cuda\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"

if (Test-Path "$cuda\bin\nvcc.exe") {
    $env:CUDA_PATH = $cuda
    $env:CUDAToolkit_ROOT = $cuda
    $env:CUDACXX = "$cuda\bin\nvcc.exe"
    $env:CMAKE_CUDA_COMPILER = "$cuda\bin\nvcc.exe"
    $env:CMAKE_CUDA_ARCHITECTURES = "89"
    $env:CMAKE_CUDA_FLAGS = "-allow-unsupported-compiler"
    # The VS generator cannot attach CUDA without its integration; Ninja + CUDACXX works.
    $env:CMAKE_GENERATOR = "Ninja"
}

$cmd = if ($args.Count -gt 0 -and $args[0] -ne "cuda") { $args[0] } else { "test" }
$rest = @($args | Where-Object { $_ -ne $cmd -and $_ -ne "cuda" })
$featureArgs = if ($useCuda) { @("--features", "cuda") } else { @() }

switch ($cmd) {
    "dev"     { Set-Location $PSScriptRoot; if ($useCuda) { npm run tauri dev -- --features cuda } else { npm run tauri dev } }
    "release" { Set-Location $PSScriptRoot; npm run tauri build -- --features cuda }
    default   {
        Set-Location (Join-Path $PSScriptRoot "src-tauri")
        & cargo $cmd @featureArgs @rest
    }
}
exit $LASTEXITCODE
