import {existsSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const env={...process.env};
for(const key of Object.keys(env))if(key.toLowerCase()==='path')delete env[key];
env.PATH=process.env.PATH;
const local=path.join(root,'.toolchain');
if(existsSync(path.join(local,'cargo/bin/cargo.exe'))){env.RUSTUP_HOME=path.join(local,'rustup');env.CARGO_HOME=path.join(local,'cargo');env.PATH=path.join(local,'cargo/bin')+path.delimiter+env.PATH;}
function run(executable,args,cwd=root){const result=spawnSync(executable,args,{cwd,env,stdio:'inherit',windowsHide:true});if(result.error){console.error(result.error.message);process.exit(1);}if(result.status!==0)process.exit(result.status??1);}
const manifests=['crates/casino-core/Cargo.toml','crates/hook-bridge/Cargo.toml','crates/agent-core/Cargo.toml','apps/desktop/src-tauri/Cargo.toml'].filter(p=>existsSync(path.join(root,p)));
const command=process.argv[2];
if(command==='test'){run(process.execPath,['apps/desktop/scripts/prepare-helper.mjs']);for(const manifest of manifests)run('cargo',['test','--locked','--manifest-path',manifest]);run('cargo',['test','--locked','--manifest-path','crates/hook-bridge/Cargo.toml','--test','registered_runner','--','--ignored']);run('cargo',['test','--locked','--manifest-path','apps/desktop/src-tauri/Cargo.toml','--','--ignored']);}
else if(command==='lint'){run(process.execPath,['apps/desktop/scripts/prepare-helper.mjs']);for(const manifest of manifests){run('cargo',['fmt','--manifest-path',manifest,'--check']);run('cargo',['clippy','--locked','--manifest-path',manifest,'--all-targets','--','-D','warnings']);}}
else if(command==='build'||command==='dev'){for(const manifest of manifests)run('cargo',['fetch','--locked','--manifest-path',manifest]);run(process.execPath,['scripts/collect-licenses.mjs']);run(process.execPath,[path.join(root,'node_modules/@tauri-apps/cli/tauri.js'),command==='build'?'build':'dev'],path.join(root,'apps/desktop'));}
else {console.error('Usage: node scripts/native.mjs test|lint|build|dev');process.exit(1);}
