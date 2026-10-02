param(
    [string]$Sdk = $env:PS5_PAYLOAD_SDK,
    [string]$Llvm = 'C:\Program Files\LLVM\bin',
    [int]$Port = 8000,
    [string]$Out = 'ps5-rpc.elf'
)
$ErrorActionPreference = 'Stop'
if (-not $Sdk) { throw 'Set PS5_PAYLOAD_SDK or pass -Sdk (the unzipped ps5-payload-sdk folder).' }

# The SDK's win\prospero-clang.cmd predates clang 20, which adds the PS5 crt files on its own:
# linking through it gives duplicate symbols. This follows bin/prospero-clang instead. The SDK dir
# also has to come from SCE_PROSPERO_SDK_DIR, or clang derives it from its own install path and
# prospero-lld.exe splits that at the space in "Program Files".
$env:SCE_PROSPERO_SDK_DIR = $Sdk
$env:PATH = "$Llvm;$Sdk\win;$env:PATH"

Push-Location $PSScriptRoot
try {
    & "$Llvm\clang.exe" -target x86_64-sie-ps5 -isysroot $Sdk -isystem "$Sdk\target\include" `
        -L "$Sdk\target\lib" -L "$Sdk\target\user\homebrew\lib" `
        -fno-stack-protector -fno-plt -femulated-tls -Wall -Werror -O2 "-DRPC_PORT=$Port" `
        -o $Out main.c -lc -lkernel_web -lSceLibcInternal -lSceNet -lSceSystemService
    if ($LASTEXITCODE -ne 0) { throw "clang failed ($LASTEXITCODE)" }
    & "$Llvm\llvm-strip.exe" $Out
    Get-Item $Out | ForEach-Object { '{0}  {1:N0} bytes' -f $_.FullName, $_.Length }
} finally {
    Pop-Location
}
