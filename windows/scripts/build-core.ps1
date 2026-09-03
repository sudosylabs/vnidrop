param([ValidateSet('debug', 'release')][string]$Profile = 'debug')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$generator = Join-Path $repo 'build/windows/tools/uniffi/bin/uniffi-bindgen-cs.exe'
Push-Location $repo
try {
    if (!(Test-Path -LiteralPath $generator)) {
        & cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --rev e4f18be96ca812571fd85fc39555646158b06b9c --locked --root (Join-Path $repo 'build/windows/tools/uniffi')
        if ($LASTEXITCODE) { throw 'C# binding generator installation failed' }
    }
    $arguments = @('build', '-p', 'vnidrop', '--target', 'x86_64-pc-windows-msvc', '--locked')
    if ($Profile -eq 'release') { $arguments += '--release' }
    & cargo @arguments
    if ($LASTEXITCODE) { throw 'Rust core build failed' }
    & $generator --library "target/x86_64-pc-windows-msvc/$Profile/vnidrop.dll" --config windows/uniffi.toml --out-dir build/windows/bindings --no-format
    if ($LASTEXITCODE) { throw 'C# binding generation failed' }
} finally { Pop-Location }
