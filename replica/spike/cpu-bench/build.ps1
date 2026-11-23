$ErrorActionPreference = "Stop"
$vs = "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
if (Test-Path $vs) {
    cmd.exe /c "`"$vs`" >nul && set" | ForEach-Object {
        if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "Env:$($matches[1])" -Value $matches[2] }
    }
}
$env:LIBCLANG_PATH = Join-Path $env:LOCALAPPDATA "Programs\Python\Python314\Lib\site-packages\clang\native"
& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release
exit $LASTEXITCODE
