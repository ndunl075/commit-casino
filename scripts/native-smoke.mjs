// Actual Windows WebView2 -> Tauri IPC -> SQLite, isolated from the user's wallet.
import {spawn, spawnSync} from 'node:child_process';
import {mkdirSync, existsSync, writeFileSync, appendFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium, expect} from '@playwright/test';
import {configureHostedWebView} from './hosted-webview-policy.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
if(process.platform!=='win32')throw Error('Native smoke requires Windows and WebView2.');
const env={...process.env};const toolchain=path.join(root,'.toolchain');
for(const key of Object.keys(env))if(key.toLowerCase()==='path')delete env[key];
env.PATH=process.env.PATH;
if(existsSync(path.join(toolchain,'cargo/bin/cargo.exe'))){env.RUSTUP_HOME=path.join(toolchain,'rustup');env.CARGO_HOME=path.join(toolchain,'cargo');env.PATH=path.join(toolchain,'cargo/bin')+path.delimiter+env.PATH;}
function run(executable,args,cwd=root){const p=spawnSync(executable,args,{cwd,env,stdio:'inherit',windowsHide:true});if(p.error||p.status!==0)throw p.error??Error(`${executable} failed (${p.status})`);}
if(!process.argv.includes('--skip-build')){run(process.execPath,['scripts/collect-licenses.mjs']);run(process.execPath,[path.join(root,'node_modules/@tauri-apps/cli/tauri.js'),'build','--no-bundle','--features','native-smoke'],path.join(root,'apps/desktop'));}
const evidence=path.join(root,'test-results/native');mkdirSync(evidence,{recursive:true});
const isolated=path.join(root,'.cache',`native-smoke-${Date.now()}`);mkdirSync(isolated,{recursive:true});
const port=19337;const executable=path.join(root,'apps/desktop/src-tauri/target/release/commit-casino.exe');
const restoreHostedPolicy=configureHostedWebView(port,path.join(isolated,'webview'));
let processHandle,browser;
async function stop(){if(browser){await browser.close().catch(()=>{});browser=undefined;}if(processHandle){const child=processHandle;processHandle=undefined;child.kill();await Promise.race([new Promise(resolve=>child.once('exit',resolve)),new Promise(resolve=>setTimeout(resolve,3000))]);}}
async function launch(expectedBalance='10,000'){processHandle=spawn(executable,['--auto-launch'],{cwd:root,env:{...env,COMMIT_CASINO_SMOKE_DATA_DIR:isolated,WEBVIEW2_USER_DATA_FOLDER:path.join(isolated,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`},stdio:['ignore','pipe','pipe'],windowsHide:true});const startupLog=path.join(evidence,'startup.log');let launchError;processHandle.stdout.on('data',chunk=>appendFileSync(startupLog,chunk));processHandle.stderr.on('data',chunk=>appendFileSync(startupLog,chunk));processHandle.on('error',error=>{launchError=error;});for(let i=0;i<300;i++){try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);break;}catch{await new Promise(resolve=>setTimeout(resolve,200));}if(launchError)throw launchError;if(processHandle.exitCode!==null)throw Error('Desktop exited before WebView2 startup: '+processHandle.exitCode+'; see '+startupLog);}if(!browser)throw Error('WebView2 CDP did not start; see '+startupLog);const context=browser.contexts()[0];let page=context.pages()[0];if(!page)page=await context.waitForEvent('page');await expect(page.getByTestId('wallet-balance')).toHaveText(expectedBalance,{timeout:20000});await page.getByRole('button',{name:'Play now',exact:true}).click();return page;}
try{
 let page=await launch();await expect(page.locator('.wheel-view canvas')).toBeVisible({timeout:15000});await expect(page.locator('.wheel-fallback')).toHaveCount(0);await page.getByRole('button',{name:'Settings',exact:true}).click();await page.getByLabel('Spin animation').selectOption('reduced-motion');await page.keyboard.press('Escape');
 await page.getByRole('button',{name:'Bet picker',exact:true}).click();await page.getByRole('searchbox').fill('straight:7');await page.getByRole('button').filter({hasText:'Straight 7'}).first().click();await page.getByRole('button',{name:'Back to table'}).click();await page.getByRole('button',{name:'SPIN',exact:false}).click();
 await expect(page.locator('.result-panel')).toContainText('net chips',{timeout:15000});await page.getByRole('button',{name:'History',exact:true}).click();await expect(page.locator('tbody tr')).toHaveCount(1);const cells=await page.locator('tbody tr td').allTextContents();const outcome=Number(cells[0]);const balance=outcome===7?'13,500':'9,900';await page.keyboard.press('Escape');await expect(page.getByTestId('wallet-balance')).toHaveText(balance);await expect(page.locator('.error-banner')).toHaveCount(0);await page.screenshot({path:path.join(evidence,'native-classic.png'),fullPage:true});
 await stop();
 page=await launch(balance);await page.getByRole('button',{name:'History',exact:true}).click();await expect(page.locator('tbody tr')).toHaveCount(1);await page.keyboard.press('Escape');
 await page.getByRole('button',{name:'Settings',exact:true}).click();await page.getByLabel('Spin animation').selectOption('normal');await page.keyboard.press('Escape');await page.getByRole('button',{name:'Rebet',exact:true}).click();await page.getByRole('button',{name:'SPIN',exact:false}).click();
 const committed=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('get_snapshot'));if(!committed.activeRound)throw Error('Expected a persisted unshown round');
 const framePacing=await page.evaluate(()=>new Promise(resolve=>{const times=[];let previous=performance.now();const start=previous;function tick(now){times.push(now-previous);previous=now;if(now-start<1000)requestAnimationFrame(tick);else resolve({frames:times.length,averageMs:times.reduce((a,b)=>a+b,0)/times.length});}requestAnimationFrame(tick);}));
 await stop();const recoveredBalance=Number(balance.replaceAll(',',''))-100+committed.activeRound.grossReturn;page=await launch(recoveredBalance.toLocaleString('en-US'));await page.getByRole('button',{name:'History',exact:true}).click();await expect(page.locator('tbody tr')).toHaveCount(2);await page.keyboard.press('Escape');await page.screenshot({path:path.join(evidence,'native-recovered.png'),fullPage:true});writeFileSync(path.join(evidence,'result.json'),JSON.stringify({platform:process.platform,passed:true,outcome,balance:recoveredBalance,framePacing,isolatedData:isolated,checks:['WebView2 bundled UI','real native commit/settle','SQLite history','settled restart','kill during normal spin and recover recorded payout exactly once']},null,2));console.log('Native WebView2 / IPC / SQLite smoke passed.');
}finally{try{await stop();}finally{restoreHostedPolicy();}}

