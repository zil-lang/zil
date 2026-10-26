// Original TS remains the independent protocol/seed/reducer reference.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdir,open,readFile,writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {resolve,join} from 'node:path';
import {supervise} from '../../tools/roam/vendor/worker.ts';
import {bfs} from '../../tools/roam/vendor/search.ts';
import {shrink} from '../../tools/roam/vendor/shrink.ts';
import {generate,contract} from '../roam/session.mjs';

const args=process.argv.slice(2);
const value=name=>args.find(a=>a.startsWith(`--${name}=`))?.slice(name.length+3);
if(args.some(a=>!['--binary=','--bridge=','--out='].some(p=>a.startsWith(p))))throw Error('unknown argument');
const root=resolve(import.meta.dirname,'../..');
const binary=resolve(value('binary')??join(root,'tools/roam-rs/target/debug/roam'));
const bridge=resolve(value('bridge')??join(root,'target/debug/examples/roam_session'));
const out=resolve(value('out')??join(root,'out/runs/roam-cross-'+Date.now()));
await mkdir(out,{recursive:false});
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
let count=0;
async function native(args,label){
    const file=join(out,label+'.stdout');const error=join(out,label+'.stderr');
    const fd=await open(file,'wx');const err=await open(error,'wx');
    try{
        const code=await new Promise((ok,no)=>{
            const child=spawn(binary,args,{stdio:['ignore',fd.fd,err.fd],cwd:root});
            child.once('error',no);child.once('close',(code,signal)=>signal?no(Error(`native signal ${signal}`)):ok(code));
        });
        return {code,stdout:await readFile(file,'utf8'),stderr:await readFile(error,'utf8')};
    }finally{await fd.close();await err.close();}
}
const trace=steps=>({format:'roam/1',contract,steps});
const check=()=>({op:'check'});const action=(op,value)=>({op,value});const limit=(name,value)=>({op:'limit',name,value});
const report={format:'zil-roam-cross-language/1',status:'error',binary_sha256:sha(await readFile(binary)),bridge_sha256:sha(await readFile(bridge)),seeds:[],interop:[],normal:{},shrink:{},boundaries:['No full Roam host, lease, browser, or explicit scheduler port claim.','Comparison uses matching compiler sources/options in debug builds; elapsed includes process startup, protocol and fsync.','Benchmark is one serial campaign per host on this machine, not a product performance gate.']};
const serial=async fn=>{const start=performance.now();const result=await fn();return {...result,elapsed_ms:performance.now()-start};};
try{
    for(const seed of [0,1,2,3,7,11,17,31,63,127,255,1024,65535,0x12345678,0xdeadbeef,0xffffffff]){
        const result=await native([`generate`,`--seed=${seed}`,'--steps=64'],`seed-${seed}`);assert.equal(result.code,0,result.stderr);
        assert.deepEqual(JSON.parse(result.stdout),generate(seed,64));report.seeds.push({seed,actions:71,status:'pass'});
    }
    const cases=[
        ['pass',trace([check(),check()]),'none',{}],
        ['invalid',trace([{op:'unknown'}]),'none',{}],
        ['unsupported-contract',{...trace([]),contract:'unknown/1'},'none',{}],
        ['unsupported-schedule',{...trace([]),schedule:[{deliver:'x'}]},'none',{}],
        ['unsupported-capability',{...trace([]),requires:['network']},'none',{}],
        ['initial-error',{...trace([]),initial:{source:'one'}},'none',{}],
        ['incomplete',trace(Array.from({length:257},check)),'none',{}],
        ['injected-fail',trace([check(),action('edit','one'),check()]),'stale-after-edit',{}],
    ];
    const tsRun=async(c,fault='none')=>supervise({command:'/bin/bash',args:['--noprofile','--norc',join(root,'tools/roam/pipe-host.sh'),process.execPath,'--experimental-transform-types','--import',join(root,'tools/roam/register.mjs'),join(root,'tests/roam/worker.mjs'),bridge,fault],trace:c,timeoutMs:10_000,maxOutputBytes:4*1024*1024});
    const rsRun=async(c,fault='none')=>supervise({command:binary,args:['worker',`--fault=${fault}`],trace:c,timeoutMs:10_000,maxOutputBytes:4*1024*1024});
    const category=r=>({status:r.status,steps:r.steps,property:r.failure?.propertyId??null,reason:r.reason??null,issues:r.issues.map(i=>[i.phase,i.name])});
    for(const [name,c,fault] of cases){
        const ts=await tsRun(c,fault),rs=await rsRun(c,fault);
        assert.deepEqual(category(rs.result),category(ts.result),name);
        assert.deepEqual(rs.journal.filter(l=>['before','after'].includes(JSON.parse(l).type)).map(JSON.parse),ts.journal.filter(l=>['before','after'].includes(JSON.parse(l).type)).map(JSON.parse),name+' journal');
        report.interop.push({name,...category(rs.result),checkpoints:rs.journal.length});
    }
    // Normal TS set is exactly the original 11+57+16 campaign (84/1311).
    let paths=0,actions=0;
    const fixed=[
        ['reuse',[check(),check()]],
        ['edits',[check(),action('edit','one'),check(),action('edit','type-error'),check(),action('edit','syntax-error'),check(),action('edit','ct'),check(),check()]],
        ['target',[check(),action('target','alternate-llvm'),check(),check(),action('target','native-llvm'),check()]],
        ['disk',[check(),action('cache','corrupt'),check(),action('cache','off'),check(),action('cache','clear'),check(),action('cache','on'),check(),check()]],
        ...['before','parse','types','verify'].map(v=>['cancel-'+v,[check(),action('cancel',v),check(),action('cancel','none'),check(),check()]]),
        ...['tokens','nodes','analysis'].map(n=>['limit-'+n,[check(),limit(n,1),check(),limit(n,n==='analysis'?10_000_000:1_000_000),check(),check()]])
    ];
    const campaign=await serial(async()=>{
        const run=async c=>{const r=await tsRun(c);assert.equal(r.result.status,'pass',JSON.stringify(r.result));paths++;actions+=r.result.steps;};
        for(const [label,steps]of fixed)await run(trace(steps));
        let path=[];
        const bounded=await bfs({init:()=>[],ops:()=>[check(),action('edit','type-error'),action('target','alternate-llvm'),action('cancel','before'),limit('tokens',1),action('cache','corrupt'),action('cache','clear')],next:(p,a)=>[...p,a],key:JSON.stringify},{reset:()=>{path=[];},fire:a=>path.push(a),chk:async()=>{await run(trace(path));}},{depth:2,cap:1000,maxBad:1});
        assert.equal(bounded.edges,56);assert.equal(bounded.complete,false);
        for(const seed of report.seeds.map(v=>v.seed))await run(generate(seed,64));
        assert.equal(paths,84);assert.equal(actions,1311);return {paths,actions,bfs_edges:bounded.edges};
    });
    report.normal.ts=campaign;
    const result=await native(['campaign',`--out=${join(out,'native-campaign')}`,'--allow-dirty=true'], 'native-campaign');
    assert.equal(result.code,0,result.stderr);const nativeReport=JSON.parse(await readFile(join(out,'native-campaign/report.json'),'utf8'));
    assert.equal(nativeReport.normal_paths,84);assert.equal(nativeReport.normal_actions,1311);
    report.normal.rust={paths:nativeReport.normal_paths,actions:nativeReport.normal_actions,elapsed_ms:nativeReport.normal_elapsed_ms,bfs_edges:nativeReport.bounded_bfs.edges};
    report.normal.observed_ratio=report.normal.ts.elapsed_ms/report.normal.rust.elapsed_ms;
    const original=trace([action('cache','on'),check(),action('target','alternate-llvm'),check(),action('cancel','none'),action('edit','type-error'),action('cache','corrupt'),check()]);
    const reduced=await shrink(original,async c=>(await rsRun(c,'stale-after-edit')).result,{maxRuns:64,budgetMs:30_000});
    const nativeReduced=JSON.parse(await readFile(join(out,'native-campaign/shrink.json'),'utf8'));
    assert.deepEqual(reduced.reduced,nativeReduced.reduced);assert.equal(reduced.runs,nativeReduced.runs);assert.equal(reduced.stopped,nativeReduced.stopped);
    report.shrink={runs:reduced.runs,steps:reduced.reduced.steps.length,stopped:reduced.stopped,property:reduced.result.failure.propertyId,ts_reducer_rust_worker:'pass'};
    report.status='pass';
}catch(e){report.error={name:e.name,message:e.message,stack:e.stack};process.exitCode=1;}
await writeFile(join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({status:report.status,report:join(out,'report.json'),seeds:report.seeds.length,interop:report.interop.length,normal:report.normal,shrink:report.shrink,error:report.error}));
