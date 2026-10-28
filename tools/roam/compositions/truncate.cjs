#!/usr/bin/env node
'use strict';
// Adapter for the EXISTING Roam core/shrink. It neither changes the queue nor
// disables Verify. Independent JS state, complete source and traces are saved.
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),crypto=require('node:crypto');
const args=Object.fromEntries(process.argv.slice(2).reduce((a,v,i,x)=>i%2?a:(a.push([v.replace(/^--/,''),x[i+1]]),a),[]));
for(const k of ['out','core','baseline','candidate'])if(!args[k])throw Error(`missing --${k}`);
const out=path.resolve(args.out);fs.mkdirSync(out,{recursive:false});
const core=require(path.join(path.resolve(args.core),'core.js')), {shrink}=require(path.join(path.resolve(args.core),'shrink.js'));
const CONTRACT='zil-byte-truncate-composition/1',PROPERTY='byte-truncate.complete-state/1';
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const write=(f,x)=>fs.writeFileSync(f,JSON.stringify(x,null,2)+'\n');
const bins={A:path.resolve(args.baseline),C:path.resolve(args.candidate)};
write(path.join(out,'plan.json'),{contract:CONTRACT,property:PROPERTY,carriers:['local','package','fnmut','fnonce'],seeds:Number(args.seeds||8),compilers:Object.fromEntries(Object.entries(bins).map(([k,v])=>[k,{path:v,sha256:sha(fs.readFileSync(v))}])),verify:'always enabled',versions:['O0','O2'],mutation:'controlled generator mutation; never a natural defect',model:'independent JS array/capacity; no compiler code imported',oom:'not injected in this profile; existing allocator tests are separate',used_helper_functions:32,helper_depth:8,graph_path:'module >=32 functions; helpers all reachable from actions'});
const PRE=`extern "C" fn putchar(c:i32)->i32;
fn emit(c:i32){unsafe{putchar(c);}}
fn digit(v:i64){if v<10{emit(i32(v+48));}else{emit(i32(v+55));}}
fn hex(v:i64){digit(v/16);digit(v%16);}
fn show(b:ByteBuffer,ok:bool){if ok{emit(84);}else{emit(70);}hex(bytes_len(b));hex(bytes_capacity(b));let w=bytes_view(b);var i:i64=0;while i<w.len(){hex(i64(w[i]));i+=1;}emit(10);}
fn fill(b:ByteBuffer,i:i64,v:u8)->bool{if i<0||i>=bytes_len(b){return false;}let w=bytes_view_mut(b);w[i]=v;return true;}
package Owner{manages data:ByteBuffer}
fn consume(give b:ByteBuffer){}
`;
function validate(c){core.validateCase(c);if(!['local','package','fnmut','fnonce'].includes(c.initial.carrier)||c.steps.length>64)throw Error('bounded carrier/steps');for(const a of c.steps){if(!['reserve','push','truncate','fill'].includes(a.op)||!Number.isInteger(a.n)||a.n< -1||a.n>32||!Number.isInteger(a.v)||a.v<0||a.v>255)throw Error('bounded operation');}}
function source(c,mutant=false){validate(c);const carrier=c.initial.carrier,b=carrier==='local'?'b':'p.data';

 const entry=a=>`route_${a.op}_0(${b},i64(${a.n}),${a.v}u8)`;
 const wrappers=['reserve','push','truncate','fill'].map(op=>Array.from({length:8},(_,i)=>`fn route_${op}_${i}(target:ByteBuffer,n:i64,v:u8)->bool{return ${i<7?`route_${op}_${i+1}(target,n,v)`:op==='reserve'?'bytes_reserve(target,n)':op==='push'?'bytes_push(target,v)':op==='fill'?'fill(target,n,v)':mutant?'bytes_truncate(target,if n>0{n-1}else{n})':'bytes_truncate(target,n)'};}\n`).join('')).join('');
 const pre=PRE+wrappers;
 const body=c.steps.map(a=>`show(${b},${entry(a)});`).join('\n');
 if(carrier==='local')return pre+`fn main()->i32{let b=bytes_new();show(b,true);${body}return 0;}`;
 if(carrier==='package')return pre+`fn main()->i32{let p=Owner{data:bytes_new()};show(p.data,true);${body}return 0;}`;
 if(carrier==='fnonce')return pre+`fn make()->impl FnOnce()->i32{let p=Owner{data:bytes_new()};return move()->i32=>{show(p.data,true);${body}let old=give p.data;consume(give old);return 0;};}fn main()->i32{let f=make();return f();}`;
 return pre+`fn make()->impl FnMut(i32)->i32{let p=Owner{data:bytes_new()};return move(command:i32)->i32=>{if command<0{show(p.data,true);return 0;}\n${c.steps.map((a,i)=>`if command==${i}{show(p.data,${entry(a)});return 0;}`).join('\n')}return 1;};}fn main()->i32{var f=make();f(-1);${c.steps.map((_,i)=>`if f(${i})!=0{return 1;}`).join('')}return 0;}`;
}
function line(m){return(m.ok?'T':'F')+[m.bytes.length,m.capacity,...m.bytes].map(v=>v.toString(16).toUpperCase().padStart(2,'0')).join('');}
const spec={contract:CONTRACT,init(){return{bytes:[],capacity:0,ok:true}},step(m,a){m.ok=false;if(a.op==='reserve'){m.ok=a.n>=0;if(m.ok)m.capacity=Math.max(m.capacity,a.n);}else if(a.op==='push'){m.ok=m.bytes.length<m.capacity;if(m.ok)m.bytes.push(a.v);}else if(a.op==='truncate'){m.ok=a.n>=0&&a.n<=m.bytes.length;if(m.ok)m.bytes.length=a.n;}else{m.ok=a.n>=0&&a.n<m.bytes.length;if(m.ok)m.bytes[a.n]=a.v;}return m;},check(m,observed){for(const [variant,value] of Object.entries(observed))core.property(PROPERTY,value===line(m),`${variant}: expected ${line(m)} actual ${value}`)}};
let serial=0,nativeRuns=0,compileRuns=0;
function run(command,argv,dir,kind){const r=cp.spawnSync(command,argv,{encoding:'utf8',timeout:30000,maxBuffer:8*1024*1024});write(path.join(dir,kind+'-command.json'),{command,argv,status:r.status,error:r.error?.message});fs.writeFileSync(path.join(dir,kind+'-stdout'),r.stdout||'');fs.writeFileSync(path.join(dir,kind+'-stderr'),r.stderr||'');if(r.error||r.status!==0)throw Error(`${kind}: ${r.error?.message||r.status}\n${(r.stdout||'').slice(0,1200)}\n${(r.stderr||'').slice(0,1000)}`);return r.stdout;}
function driver(mutant){return{init(initial,ctx){const dir=path.join(out,String(serial++).padStart(4,'0'));fs.mkdirSync(dir);write(path.join(dir,'case.json'),ctx.case);const src=source(ctx.case,mutant),file=path.join(dir,'main.zil');fs.writeFileSync(file,src);const lines={};for(const variant of Object.keys(bins))for(const opt of ['O0','O2']){const k=variant+'-'+opt,d=path.join(dir,k);fs.mkdirSync(d);const exe=path.join(d,'program');const report=JSON.parse(run(bins[variant],['build',file,'--json','--opt',opt,'-o',exe],d,'compile'));compileRuns++;core.property('byte-truncate.verify-called/1',report.verification_runs===1&&report.status==='Success');const text=run(exe,[],d,'native');nativeRuns++;lines[k]=text.trimEnd().split('\n');core.property('byte-truncate.record-count/1',lines[k].length===ctx.case.steps.length+1);}
 return{index:0,lines,dir};},step(r){r.index++;},observe(r){return Object.fromEntries(Object.entries(r.lines).map(([k,v])=>[k,v[r.index]]));}};}
function generate(seed,carrier){let s=(seed+1)>>>0;const rnd=n=>{s=(Math.imul(s,1664525)+1013904223)>>>0;return s%n};const steps=[{op:'reserve',n:8,v:0},{op:'push',n:0,v:65},{op:'push',n:0,v:66},{op:'truncate',n:1,v:0},{op:'truncate',n:3,v:0},{op:'truncate',n:-1,v:0}];for(let i=0;i<10;i++){const op=['reserve','push','truncate','fill'][rnd(4)];steps.push({op,n:rnd(15)-1,v:rnd(256)});}return{format:'roam/1',contract:CONTRACT,initial:{carrier},steps,seed,meta:{generator:'truncate-composition/1'}};}
(async()=>{const results=[];for(let seed=0;seed<Number(args.seeds||8);seed++)for(const carrier of ['local','package','fnmut','fnonce']){const c=generate(seed,carrier),r=await core.execute(spec,driver(false),c,{history:true});results.push({seed,carrier,result:r});write(path.join(out,'results.json'),results);console.log(seed,carrier,r.status);if(r.status!=='pass')throw Error('natural run failed; preserved record');}
 const injected=generate(99,'package'), replay=c=>core.execute(spec,driver(true),c,{history:true});const shrunk=await shrink(injected,replay,{maxRuns:24,budgetMs:90000});write(path.join(out,'controlled-shrink.json'),shrunk);core.property('control-target',shrunk.result.status==='fail'&&shrunk.result.failure.propertyId===PROPERTY);write(path.join(out,'summary.json'),{status:'Pass',cases:results.length,steps:results.reduce((n,x)=>n+x.result.steps,0),natural_defects:0,controlled_mutations:1,controlled_property:PROPERTY,shrink_runs:shrunk.runs,original_steps:injected.steps.length,reduced_steps:shrunk.reduced.steps.length,shrink_stop:shrunk.stopped,compileRuns,nativeRuns,verify_disabled:false,tooling:'existing TS Roam core/shrink, not newly rebuilt Rust Roam'});
})().catch(e=>{write(path.join(out,'failure.json'),{message:e.message,stack:e.stack,compileRuns,nativeRuns});console.error(e);process.exitCode=1;});
