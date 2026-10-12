import React,{useEffect,useState} from 'react';
import {createRoot} from 'react-dom/client';
import {CasinoApp} from '@commit-casino/game-ui';
import {BrowserCasinoService} from '@commit-casino/browser-runtime';

function App(){const [service,setService]=useState<BrowserCasinoService|null>(null);const [error,setError]=useState('');const [temporary,setTemporary]=useState(false);
 const open=async(temp=false)=>{setError('');try{const next=await BrowserCasinoService.create({temporary:temp});setService(old=>{old?.close();return next;});setTemporary(temp);}catch(e){setError(e instanceof Error?e.message:'Your saved wallet is unavailable.');}};
 useEffect(()=>{void open();if('serviceWorker' in navigator&&import.meta.env.PROD)void navigator.serviceWorker.register('./service-worker.js').catch(()=>undefined);},[]);
 if(!service)return <main style={{minHeight:'100vh',background:'#111615',color:'#f1eee6',padding:48,fontFamily:'system-ui'}}><h1>Commit Casino</h1>{error?<><h2>Saved play is unavailable</h2><p role="alert">{error}</p><p>Your existing wallet has not been erased. Retry after storage is available, or play with a temporary wallet that will disappear when you close this page.</p><button onClick={()=>void open()}>Retry saved play</button><button onClick={()=>void open(true)}>Use temporary play</button></>:<p role="status">Opening your table…</p>}</main>;
 return <CasinoApp service={service} temporary={temporary} onTemporaryPlay={()=>void open(true)}/>;
}
createRoot(document.getElementById('root')!).render(<App/>);
