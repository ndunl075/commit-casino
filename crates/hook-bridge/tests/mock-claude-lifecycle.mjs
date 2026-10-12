/** Developer acceptance harness only. No provider API, user configuration, source
 * repository or real model is used. CLI lifecycle real; model responses mocked. */
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const crate = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const executable = process.argv[2];
const helperOriginal = process.argv[3] ?? path.join(crate,'target','debug','commit-casino-bridge.exe');
if (!executable || !path.isAbsolute(executable)) throw new Error('Explicit absolute installed Claude executable required');
const parent=path.join(crate,'target','mock-lifecycle');fs.mkdirSync(parent,{recursive:true});
const temporary=fs.mkdtempSync(path.join(parent,'space 空格-'));
const helper=path.join(temporary,'bridge space 空格.exe');fs.copyFileSync(helperOriginal,helper);
const working=path.join(temporary,'project');fs.mkdirSync(working);
const providerConfig=path.join(temporary,'provider-config');fs.mkdirSync(providerConfig);
const fixture=path.join(working,'synthetic-fixture.txt');fs.writeFileSync(fixture,'Synthetic local lifecycle fixture. No user source.\n');
const env={};
for(const key of ['PATH','SystemRoot','WINDIR','COMSPEC','PATHEXT','TEMP','TMP']) if(process.env[key]) env[key]=process.env[key];
Object.assign(env,{LOCALAPPDATA:temporary,APPDATA:temporary,USERPROFILE:temporary,HOME:temporary,CLAUDE_CONFIG_DIR:providerConfig,ANTHROPIC_API_KEY:'local-mock-not-a-real-key',ANTHROPIC_AUTH_TOKEN:'',CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC:'1',DISABLE_AUTOUPDATER:'1',DISABLE_TELEMETRY:'1',DISABLE_ERROR_REPORTING:'1',NO_PROXY:'127.0.0.1,localhost'});
const version=spawnSync(executable,['--version'],{env,encoding:'utf8',timeout:3000});
if(version.status!==0 || !version.stdout.startsWith('2.1.287 ')) throw new Error('Expected observed passive CLI version2.1.287');
const initialized=spawnSync(helper,['init','2.1.287'],{env,encoding:'utf8',timeout:3000});if(initialized.status!==0)throw new Error('Private integration initialization failed');
const preview=spawnSync(helper,['preview'],{env,encoding:'utf8',timeout:3000});if(preview.status!==0)throw new Error('Preview failed');
const settings=path.join(temporary,'settings.json');fs.writeFileSync(settings,JSON.stringify({hooks:Object.fromEntries(JSON.parse(preview.stdout).map(([name,entry])=>[name,[entry]]))}));
let caseName='read',messageCount=0,externalProxyAttempts=0;
function response(stream,tool){
  const isRead=caseName.startsWith('read');
  const content=tool?[{type:'tool_use',id:`toolu_local_${caseName}`,name:isRead?'Read':'Bash',input:isRead?{file_path:caseName==='read'?fixture:path.join(working,'missing-synthetic-fixture.txt')}:{command:'echo synthetic > permission-probe.txt',description:'Synthetic local permission lifecycle check'}}]:[{type:'text',text:'Local mock lifecycle complete.'}];
  const message={id:`msg_local_${messageCount}`,type:'message',role:'assistant',model:'claude-sonnet-4-6',content,stop_reason:tool?'tool_use':'end_turn',stop_sequence:null,usage:{input_tokens:1,output_tokens:10}};
  if(!stream)return JSON.stringify(message);
  const start={...message,content:[],stop_reason:null,usage:{input_tokens:1,output_tokens:0}};
  const events=[['message_start',{type:'message_start',message:start}]];
  events.push(['content_block_start',{type:'content_block_start',index:0,content_block:tool?{type:'tool_use',id:content[0].id,name:content[0].name,input:{}}:{type:'text',text:''}}]);
  events.push(['content_block_delta',{type:'content_block_delta',index:0,delta:tool?{type:'input_json_delta',partial_json:JSON.stringify(content[0].input)}:{type:'text_delta',text:content[0].text}}]);
  events.push(['content_block_stop',{type:'content_block_stop',index:0}],['message_delta',{type:'message_delta',delta:{stop_reason:message.stop_reason,stop_sequence:null},usage:{output_tokens:10}}],['message_stop',{type:'message_stop'}]);
  return events.map(([event,data])=>`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('');
}
const server=http.createServer(async(req,res)=>{
  if(req.url.startsWith('http')){externalProxyAttempts++;res.writeHead(403);res.end();return;}
  if(req.url.includes('count_tokens')){res.setHeader('content-type','application/json');res.end(JSON.stringify({input_tokens:1}));return;}
  if(!req.url.startsWith('/v1/messages')){res.writeHead(404);res.end('{}');return;}
  let bytes=0,body='';for await(const chunk of req){bytes+=chunk.length;if(bytes>1024*1024){res.writeHead(413);res.end();return;}body+=chunk;}
  const stream=JSON.parse(body).stream===true;body='';messageCount++;
  res.writeHead(200,{'content-type':stream?'text/event-stream':'application/json'});res.end(response(stream,messageCount===1));
});
server.on('connect',(req,socket)=>{externalProxyAttempts++;socket.end('HTTP/1.1 403 Forbidden\r\n\r\n');});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const port=server.address().port;Object.assign(env,{ANTHROPIC_BASE_URL:`http://127.0.0.1:${port}`,HTTPS_PROXY:`http://127.0.0.1:${port}`,HTTP_PROXY:`http://127.0.0.1:${port}`});
const summaries=[];
try{
  for(const name of ['read','read-failure','permission']){
    caseName=name;messageCount=0;
    if(name==='permission'){const scoped=JSON.parse(fs.readFileSync(settings,'utf8'));scoped.permissions={ask:['Bash']};fs.writeFileSync(settings,JSON.stringify(scoped));}
    const args=['--print','--model','claude-sonnet-4-6','--setting-sources','','--settings',settings,'--strict-mcp-config','--mcp-config','{"mcpServers":{}}','--no-session-persistence','--tools',name.startsWith('read')?'Read':'Bash','--output-format','json'];
    const child=spawn(executable,args,{cwd:working,env,stdio:['pipe','pipe','pipe'],windowsHide:true});
    let stdoutLength=0,stderrLength=0;child.stdout.on('data',chunk=>stdoutLength+=chunk.length);child.stderr.on('data',chunk=>stderrLength+=chunk.length);
    child.stdin.end('Run the synthetic local lifecycle check. Only the private fixture is relevant.');
    const timeout=setTimeout(()=>child.kill(),30000);
    const exitCode=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',resolve);});clearTimeout(timeout);
    summaries.push({case:name,exitCode,localMockRequests:messageCount,stdoutLength,stderrLength});
  }
}finally{await new Promise(resolve=>server.close(resolve));}
const root=path.join(temporary,'CommitCasino','integration');const config=JSON.parse(fs.readFileSync(path.join(root,'config.json'),'utf8'));
const sessions=new Map();
for(const filename of fs.readdirSync(path.join(root,'inbox'))){
  if(!filename.endsWith('.json'))continue;
  const signed=JSON.parse(fs.readFileSync(path.join(root,'inbox',filename),'utf8'));
  const signature=crypto.createHmac('sha256',Buffer.from(config.secret,'hex')).update(signed.payload).digest('hex');
  if(signature!==signed.signature)throw new Error('Signature validation failed');
  const event=JSON.parse(signed.payload);const events=sessions.get(event.observation.sessionId)??[];events.push({sequence:event.sourceSequence,type:event.observation.type,effect:event.observation.effect,toolStatus:event.observation.toolStatus});sessions.set(event.observation.sessionId,events);
  const allowed=new Set(['sessionId','type','toolId','effect','attention','toolStatus']);
  if(Object.keys(event.observation).some(key=>!allowed.has(key))||signed.payload.includes(fixture)||signed.payload.includes('synthetic local lifecycle check'))throw new Error('Hook metadata privacy contract violated');
}
const observed=[...sessions.values()].map(events=>events.sort((a,b)=>a.sequence-b.sequence));
const diagnosticPath=path.join(root,'hook-diagnostic.json');
const result={providerVersion:'2.1.287',modelResponses:'LOCAL MOCK; no external inference',isolated:true,summaries,externalProxyAttempts,sessions:observed,pendingHooks:fs.readdirSync(path.join(root,'pending-hooks')).length,diagnostic:fs.existsSync(diagnosticPath)?JSON.parse(fs.readFileSync(diagnosticPath,'utf8')).code:null};
console.log(JSON.stringify(result,null,2));
if(summaries.some(x=>x.exitCode!==0||x.localMockRequests!==2)||result.pendingHooks!==0||result.externalProxyAttempts!==0||result.diagnostic!==null||observed.some(events=>events.some((event,index)=>event.sequence!==index+1))||!observed.some(events=>events.some(e=>e.type==='tool-end'&&e.toolStatus==='success'))||!observed.some(events=>events.some(e=>e.type==='tool-end'&&e.toolStatus==='failure'))||!observed.some(events=>events.some(e=>e.type==='attention-requested')))process.exitCode=1;
