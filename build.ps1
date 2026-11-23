$ErrorActionPreference = "Stop"

$vs = "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
if (Test-Path $vs) {
    cmd.exe /c "`"$vs`" >nul && set" | ForEach-Object {
        if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "Env:$($matches[1])" -Value $matches[2] }
    }
}

$python = Join-Path $env:LOCALAPPDATA "Programs\Python\Python314"
$scripts = Join-Path $python "Scripts"
$libclang = Join-Path $python "Lib\site-packages\clang\native"
$cuda = "C:\cuda-env\Library"

$env:PATH = "$scripts;$cuda\bin;$env:PATH"
if (Test-Path $libclang) {
    $env:LIBCLANG_PATH = $libclang
}

$env:GGML_NATIVE = "OFF"
$env:GGML_AVX = "ON"
$env:GGML_AVX2 = "ON"
$env:GGML_FMA = "ON"
$env:GGML_F16C = "ON"
$env:GGML_SSE42 = "ON"
$env:CMAKE_C_FLAGS_RELEASE = "/O2 /Ob2 /DNDEBUG"
$env:CMAKE_CXX_FLAGS_RELEASE = "/O2 /Ob2 /DNDEBUG"
$env:CMAKE_GENERATOR = "Ninja"

if (Test-Path "$cuda\bin\nvcc.exe") {
    $env:CUDA_PATH = $cuda
    $env:CUDAToolkit_ROOT = $cuda
    $env:CUDACXX = "$cuda\bin\nvcc.exe"
    $env:CMAKE_CUDA_COMPILER = "$cuda\bin\nvcc.exe"
    $env:CMAKE_CUDA_ARCHITECTURES = "89"
    $env:CMAKE_CUDA_FLAGS = "-allow-unsupported-compiler"
}

& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release @args
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = "$PSScriptRoot\target\release\meeting-copilot.exe"
foreach ($dll in @("cudart64_12.dll", "cublas64_12.dll", "cublasLt64_12.dll")) {
    $src = "$cuda\bin\$dll"
    if (Test-Path $src) { Copy-Item $src "$PSScriptRoot\target\release\" -Force }
}
Write-Output "built: $exe"
