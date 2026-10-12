import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { CasinoService, CasinoSnapshot, CommitClassicRequest, SettingsPatch } from '@commit-casino/casino-contracts';
import type { IntegrationPort } from '@commit-casino/game-ui';

const listeners = new Set<(snapshot: CasinoSnapshot) => void>();
let revision = -1;
function publish(snapshot:CasinoSnapshot) { if(snapshot.revision <= revision)return;revision=snapshot.revision;for(const listener of listeners)listener(snapshot); }
const ready = listen<CasinoSnapshot>('casino://snapshot',event=>publish(event.payload));
async function snapshotCommand(command:string,args?:Record<string,unknown>):Promise<CasinoSnapshot>{await ready;const snapshot=await invoke<CasinoSnapshot>(command,args);publish(snapshot);return snapshot;}
export const desktopService: CasinoService = {
 getSnapshot: () => snapshotCommand('get_snapshot'),
 commitClassic: (request:CommitClassicRequest) => invoke('commit_classic',{request}),
 acknowledgePresentation: (roundId:string) => invoke('acknowledge_presentation',{roundId}),
 refillWallet: (requestId:string) => snapshotCommand('refill_wallet',{requestId}),
 resetWallet: (requestId:string) => snapshotCommand('reset_wallet',{requestId}),
 clearHistory: (requestId:string) => snapshotCommand('clear_history',{requestId}),
 updateSettings: (patch:SettingsPatch) => snapshotCommand('update_settings',{patch}),
 subscribe(listener) {listeners.add(listener);void snapshotCommand('get_snapshot').then(listener);return ()=>{listeners.delete(listener);};},
};
document.addEventListener('visibilitychange',()=>{if(!document.hidden)void snapshotCommand('get_snapshot');});
export const desktopIntegration: IntegrationPort = {
 status:()=>invoke('integration_status'),
 preview:(projectPath:string)=>invoke('preview_integration',{projectPath}),
 install:(projectPath:string)=>invoke('install_integration',{projectPath}),
 disconnect:(projectPath:string)=>invoke('disconnect_integration',{projectPath}),
 check:()=>invoke('integration_check'),
};
