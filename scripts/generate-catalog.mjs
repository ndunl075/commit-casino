import {mkdirSync,writeFileSync,readFileSync} from 'node:fs';
const wheelOrder=[0,32,15,19,4,21,2,25,17,34,6,27,13,36,11,30,8,23,10,5,24,16,33,1,20,14,31,9,22,18,29,7,28,12,35,3,26];
const redNumbers=[1,3,5,7,9,12,14,16,18,19,21,23,25,27,30,32,34,36];
const bets=[];
const add=(kind,numbers,profitOdds,id)=>{numbers.sort((a,b)=>a-b);bets.push({id:id??`${kind}:${numbers.join('-')}`,kind,numbers,profitOdds});};
for(let n=0;n<=36;n++) add('straight',[n],35);
for(let n=1;n<=36;n++){if(n%3!==0)add('split',[n,n+1],17);if(n<=33)add('split',[n,n+3],17);}
for(let n=1;n<=3;n++)add('split',[0,n],17);
for(let n=1;n<=34;n+=3)add('street',[n,n+1,n+2],11);
add('trio',[0,1,2],11);add('trio',[0,2,3],11);
for(let n=1;n<=32;n++)if(n%3!==0)add('corner',[n,n+1,n+3,n+4],8);
add('first-four',[0,1,2,3],8);
for(let n=1;n<=31;n+=3)add('six-line',Array.from({length:6},(_,i)=>n+i),5);
for(let i=0;i<3;i++)add('dozen',Array.from({length:12},(_,n)=>i*12+n+1),2,`dozen:${i+1}`);
for(let i=1;i<=3;i++)add('column',Array.from({length:12},(_,n)=>n*3+i),2,`column:${i}`);
const nonzero=Array.from({length:36},(_,i)=>i+1);
for(const [name,numbers] of Object.entries({red:redNumbers,black:nonzero.filter(n=>!redNumbers.includes(n)),odd:nonzero.filter(n=>n%2),even:nonzero.filter(n=>n%2===0),low:nonzero.slice(0,18),high:nonzero.slice(18)}))add('outside',[...numbers],1,`outside:${name}`);
if(bets.length!==157)throw Error(`Expected 157 bets, got ${bets.length}`);
mkdirSync('rules',{recursive:true});
const output=JSON.stringify({version:'european-v1',wheelOrder,redNumbers,bets},null,2)+'\n';
if(process.argv.includes('--check')){if(readFileSync('rules/european-v1.json','utf8').replaceAll('\r\n','\n')!==output)throw Error('Catalog drift');}else writeFileSync('rules/european-v1.json',output);

