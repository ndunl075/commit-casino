$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskVersion = (Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json).version
$taskOutput = Join-Path $taskRoot "dist/release/$taskVersion"
New-Item -ItemType Directory -Force -Path $taskOutput | Out-Null
$taskInstaller = Join-Path $taskRoot "apps/desktop/src-tauri/target/release/bundle/nsis/Commit Casino_${taskVersion}_x64-setup.exe"
if (-not (Test-Path -LiteralPath $taskInstaller)) { throw 'Build the Windows installer first: npm run build:desktop' }
Copy-Item -LiteralPath $taskInstaller -Destination $taskOutput
Copy-Item -LiteralPath (Join-Path $taskRoot 'crates/hook-bridge/target/release/commit-casino-bridge.exe') -Destination $taskOutput
Compress-Archive -Path (Join-Path $taskRoot 'apps/web/dist/*') -DestinationPath (Join-Path $taskOutput "commit-casino-web-$taskVersion.zip") -Force
Compress-Archive -Path (Join-Path $taskRoot 'dist/licenses/*') -DestinationPath (Join-Path $taskOutput "commit-casino-licenses-$taskVersion.zip") -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'docs/RELEASE_NOTES.md') -Destination $taskOutput
$taskChecksums = Get-ChildItem -LiteralPath $taskOutput -File | Where-Object Name -ne 'SHA256SUMS.txt' | Sort-Object Name | ForEach-Object { "$((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $($_.Name)" }
Set-Content -LiteralPath (Join-Path $taskOutput 'SHA256SUMS.txt') -Value $taskChecksums -Encoding utf8NoBOM
Write-Output "Release files: $taskOutput"
