$ErrorActionPreference = 'Stop'
# Hosted Windows runners do not guarantee the Evergreen runtime. Product installers
# bundle the offline prerequisite; direct executable smoke tests need it first.
# Microsoft distribution instructions: https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution
function Get-CasinoWebViewVersion {
    foreach ($taskHive in @('HKLM:\SOFTWARE\WOW6432Node', 'HKLM:\SOFTWARE', 'HKCU:\SOFTWARE')) {
        $taskKey = "$taskHive\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
        $taskVersion = (Get-ItemProperty -LiteralPath $taskKey -ErrorAction SilentlyContinue).pv
        if ($taskVersion -and $taskVersion -ne '0.0.0.0') { return $taskVersion }
    }
}
$taskExisting = Get-CasinoWebViewVersion
if ($taskExisting) { Write-Output "WebView2 runtime already installed: $taskExisting"; exit 0 }
$taskInstaller = Join-Path $env:TEMP 'commit-casino-webview2-bootstrapper.exe'
Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $taskInstaller
$taskSignature = Get-AuthenticodeSignature -LiteralPath $taskInstaller
if ($taskSignature.Status -ne 'Valid' -or $taskSignature.SignerCertificate.Subject -notmatch 'Microsoft Corporation') { throw 'WebView2 bootstrapper signature was not valid Microsoft code' }
$taskProcess = Start-Process -FilePath $taskInstaller -ArgumentList '/silent', '/install' -WindowStyle Hidden -Wait -PassThru
for ($taskAttempt = 0; $taskAttempt -lt 30; $taskAttempt++) {
    $taskInstalled = Get-CasinoWebViewVersion
    if ($taskInstalled) { Write-Output "WebView2 runtime installed: $taskInstalled"; exit 0 }
    Start-Sleep -Seconds 2
}
throw "WebView2 runtime unavailable after setup (exit $($taskProcess.ExitCode))"
