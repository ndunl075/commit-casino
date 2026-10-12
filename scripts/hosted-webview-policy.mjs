// WebView2 150+ ignores environment overrides in elevated hosted-runner apps.
// Use app-specific HKLM policy only on disposable GitHub runners, restoring it
// after the test. Local runs retain ordinary environment-based isolation.
// https://learn.microsoft.com/microsoft-edge/webview2/concepts/security
import {execFileSync} from 'node:child_process';
function powershell(script,env){return execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-EncodedCommand',Buffer.from(script,'utf16le').toString('base64')],{env,windowsHide:true,encoding:'utf8'}).trim();}
export function configureHostedWebView(port,profile){
 if(process.env.GITHUB_ACTIONS!=='true'||process.env.CI!=='true'||process.env.RUNNER_ENVIRONMENT!=='github-hosted')return()=>{};
 const env={...process.env,CASINO_TEST_CDP_PORT:String(port),CASINO_TEST_WEBVIEW_PROFILE:profile};
 const prior=powershell(`$ErrorActionPreference='Stop'
$root='HKLM:\\Software\\Policies\\Microsoft\\Edge\\WebView2'
$states=@()
foreach($name in @('AdditionalBrowserArguments','UserDataFolder')) {
 $key=Join-Path $root $name
 $item=Get-Item -LiteralPath $key -ErrorAction SilentlyContinue
 $exists=$item -and ($item.GetValueNames() -contains 'commit-casino.exe')
 $states+=@{name=$name;exists=[bool]$exists;value=$(if($exists){$item.GetValue('commit-casino.exe')});kind=$(if($exists){$item.GetValueKind('commit-casino.exe').ToString()})}
}
ConvertTo-Json -InputObject $states -Compress`,env);
 const restore=()=>powershell(`$ErrorActionPreference='Stop'
$states=ConvertFrom-Json $env:CASINO_TEST_PRIOR_POLICY
foreach($state in $states) {
 $key=Join-Path 'HKLM:\\Software\\Policies\\Microsoft\\Edge\\WebView2' $state.name
 if($state.exists){New-ItemProperty -LiteralPath $key -Name 'commit-casino.exe' -Value $state.value -PropertyType $state.kind -Force|Out-Null}
 elseif(Test-Path -LiteralPath $key){Remove-ItemProperty -LiteralPath $key -Name 'commit-casino.exe' -ErrorAction SilentlyContinue}
}`,{...env,CASINO_TEST_PRIOR_POLICY:prior});
 try{powershell(`$ErrorActionPreference='Stop'
$root='HKLM:\\Software\\Policies\\Microsoft\\Edge\\WebView2'
foreach($name in @('AdditionalBrowserArguments','UserDataFolder')){$key=Join-Path $root $name;if(!(Test-Path -LiteralPath $key)){New-Item -Path $key -Force|Out-Null}}
New-ItemProperty -LiteralPath (Join-Path $root 'AdditionalBrowserArguments') -Name 'commit-casino.exe' -Value "--remote-debugging-port=$env:CASINO_TEST_CDP_PORT" -PropertyType String -Force|Out-Null
New-ItemProperty -LiteralPath (Join-Path $root 'UserDataFolder') -Name 'commit-casino.exe' -Value $env:CASINO_TEST_WEBVIEW_PROFILE -PropertyType String -Force|Out-Null
Write-Output 'Hosted WebView2 test-only app policy configured.'`,env);}catch(error){restore();throw error;}
 return restore;
}
