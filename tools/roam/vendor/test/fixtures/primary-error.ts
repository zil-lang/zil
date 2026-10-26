import{mock}from'bun:test';
import*as native from'node:child_process';
const actual={...native};
Object.defineProperty(process,'platform',{value:'win32'});
mock.module('node:child_process',()=>({...actual,execFile:(command:any,args:any,options:any,callback:any)=>{if(command==='taskkill'){setTimeout(()=>callback(new Error('injected tree tool failure'),'','injected unavailable tree tool'),1200);return {};}return actual.execFile(command,args,options,callback);}}));
const{supervise}=await import('../../worker');
const start=Date.now();const output=await supervise({command:process.execPath,args:['-e',"console.log('not-protocol');setInterval(()=>{},1000)"],trace:{format:'roam/1',contract:'primary/1',steps:[]},timeoutMs:1000});
console.log(JSON.stringify({result:output.result,elapsed:Date.now()-start}));
