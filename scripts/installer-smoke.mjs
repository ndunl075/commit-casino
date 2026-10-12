// Install, launch with no developer tools on PATH, then uninstall the owned test install.
import {spawn,spawnSync} from 'node:child_process';
import {existsSync,mkdirSync,writeFileSync,readFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium,expect} from '@playwright/test';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
if(process.platform!=='win32')throw Error('Installer smoke requires Windows.');
const directory=path.join(root,'.cache',`installer-smoke-${Date.now()}`);
const version=JSON.parse(readFileSync(path.join(root,'package.json'),'utf8')).version;
const setup=path.join(root,`apps/desktop/src-tauri/target/release/bundle/nsis/Commit Casino_${version}_x64-setup.exe`);
const installed=spawnSync(setup,['/S',`/D=${directory}`],{windowsHide:true,stdio:'inherit'});
if(installed.error||installed.status!==0)throw installed.error??Error('Installer failed');
let child,browser;
try{
 for(const relative of ['commit-casino.exe','commit-casino-bridge.exe','licenses/inventory.json','licenses/COMMIT-CASINO-LICENSE.txt'])if(!existsSync(path.join(directory,relative)))throw Error(`Installed resource missing: ${relative}`);
 const env={...process.env};for(const key of Object.keys(env))if(key.toLowerCase()==='path')delete env[key];env.PATH=path.join(process.env.SystemRoot??'C:/Windows','System32');env.WEBVIEW2_USER_DATA_FOLDER=path.join(directory,'smoke-webview');env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=19338';
 child=spawn(path.join(directory,'commit-casino.exe'),['--auto-launch'],{env,windowsHide:true,stdio:'ignore'});
 for(let i=0;i<100;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:19338');break;}catch{await new Promise(resolve=>setTimeout(resolve,200));}}
 if(!browser)throw Error('Installed app did not start');const context=browser.contexts()[0];const page=context.pages()[0]??await context.waitForEvent('page');await expect(page.getByTestId('wallet-balance')).toBeVisible({timeout:20000});await page.getByRole('button',{name:'Play now',exact:true}).click();await expect(page.locator('.wheel-view canvas')).toBeVisible({timeout:15000});await expect(page.locator('.wheel-fallback')).toHaveCount(0);await expect(page.locator('.error-banner')).toHaveCount(0);const evidence=path.join(root,'test-results/native');mkdirSync(evidence,{recursive:true});await page.screenshot({path:path.join(evidence,'installed-app.png'),fullPage:true});writeFileSync(path.join(evidence,'installer.json'),JSON.stringify({passed:true,developerToolsOnRuntimePath:false,resources:['UI','compiled helper','license inventory','project license'],installDirectory:directory},null,2));console.log('Installed application starts with no developer tools on PATH.');
}finally{
 if(browser)await browser.close().catch(()=>{});if(child){child.kill();await Promise.race([new Promise(resolve=>child.once('exit',resolve)),new Promise(resolve=>setTimeout(resolve,3000))]);}
 const uninstaller=path.join(directory,'uninstall.exe');if(existsSync(uninstaller)){const result=spawnSync(uninstaller,['/S'],{windowsHide:true,stdio:'inherit'});if(result.error||result.status!==0)throw result.error??Error('Uninstall failed');}
}
