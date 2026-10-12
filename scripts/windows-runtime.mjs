import {readFileSync} from 'node:fs';
// Audit actual release imports: Windows 10/11 provide the UCRT, while the
// separate Visual C++ runtime must be linked into our shipped executables.
export function assertSystemRuntime(file){
 if(process.platform!=='win32')return[];
 const bytes=readFileSync(file);
 const pe=bytes.readUInt32LE(60),count=bytes.readUInt16LE(pe+6),optional=pe+24;
 const sections=optional+bytes.readUInt16LE(pe+20);
 const directory=optional+(bytes.readUInt16LE(optional)===0x20b?112:96);
 function offset(rva){
  for(let i=0;i<count;i++){
   const section=sections+i*40,address=bytes.readUInt32LE(section+12);
   const length=Math.max(bytes.readUInt32LE(section+8),bytes.readUInt32LE(section+16));
   if(rva>=address&&rva<address+length)return bytes.readUInt32LE(section+20)+rva-address;
  }
  throw Error(`Invalid PE import address in ${file}`);
 }
 const imports=[];
 for(let entry=offset(bytes.readUInt32LE(directory+8));bytes.readUInt32LE(entry+12);entry+=20){
  const name=offset(bytes.readUInt32LE(entry+12));
  imports.push(bytes.toString('ascii',name,bytes.indexOf(0,name)));
 }
 const external=imports.filter(name=>/^(?:vcruntime|msvcp|msvcr|concrt)\d.*\.dll$/i.test(name));
 if(external.length)throw Error(`${file} needs an unbundled Visual C++ runtime: ${external.join(', ')}`);
 return imports;
}
