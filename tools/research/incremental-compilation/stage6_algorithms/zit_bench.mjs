// Exclusive-window mechanism benchmark, not a compiler benchmark.
import fs from 'node:fs/promises';
import {stripTypeScriptTypes} from 'node:module';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {performance} from 'node:perf_hooks';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
import {adapt,prelude} from './zit_adapter.mjs';
const args=process.argv.slice(2),sha=b=>createHash('sha256').update(b).digest('hex');
const median=values=>{const a=[...values].sort((x,y)=>x-y),m=Math.floor(a.length/2);return a.length%2?a[m]:(a[m-1]+a[m])/2;};

if(args[0]==='--worker') {
    const [,input,graphPath,arm,editsArg]=args,edits=Number(editsArg),fixed=arm==='adapted';
    assert.ok(edits>=2);const bytes=await fs.readFile(input),graph=JSON.parse(await fs.readFile(graphPath));
    assert.equal(sha(bytes),'719a914356a24ac1edbeb1ffe6637d22311c9299e6be9ddbcdf3f7fdddf1474f');
    const source=fixed?adapt(bytes.toString()):prelude+bytes.toString().split('\n').filter(l=>!l.startsWith('import ')).join('\n');
    const directory=await fs.mkdtemp(path.join(tmpdir(),'zil-zit-bench-'));
    try {
        const file=path.join(directory,'kernel.mjs');await fs.writeFile(file,stripTypeScriptTypes(source,{mode:'transform'}));
        const z=await import(pathToFileURL(file)),errors=[];z.$hk.err=e=>errors.push(e.message);
        const delta=[];const edit=graph.edit_node??0;
        for(let i=0;i<graph.nodes.length;i++){assert.ok(graph.nodes[i].deps.every(d=>d<i&&d>=0));let value=i===edit?1:0;for(const d of graph.nodes[i].deps)value=(value+delta[d])>>>0;delta.push(value);}
        global.gc?.();
        const signals=[],inputs=[],runs=Array(graph.nodes.length).fill(0),start=performance.now();
        for(let i=0;i<graph.nodes.length;i++) {
            inputs[i]=z.sig(0);const n=graph.nodes[i];
            signals[i]=z.sig(()=>{runs[i]++;let value=inputs[i].$;for(const d of n.deps)value=(value+signals[d].$)>>>0;return value;});
            assert.equal(signals[i].$,0);
        }
        const coldMs=performance.now()-start,initialStats={...z.adapterStats},beforeRuns=runs.reduce((a,b)=>a+b,0);
        const hotStart=performance.now();let checksum=0;
        for(let i=0;i<edits;i++){const value=(i&1)+1;inputs[edit].$=value;z.flush();const actual=signals.at(-1).$;assert.equal(actual,Math.imul(delta.at(-1),value)>>>0);checksum=(checksum+actual)>>>0;}
        const editMs=performance.now()-hotStart;
        const editStats=z.adapterStats?Object.fromEntries(Object.entries(z.adapterStats).map(([k,v])=>[k,v-(initialStats[k]??0)])):undefined;
        const editRuns=runs.reduce((a,b)=>a+b,0)-beforeRuns;
        for(let i=0;i<signals.length;i++)assert.equal(signals[i].$,Math.imul(delta[i],((edits-1)&1)+1)>>>0);
        assert.deepEqual(errors,[]);
        // Failure costs have different contracts: original mutates failed
        // dependencies; adapted restores prior successful edges. No ratio.
        const mode=z.sig(false),a=z.sig(10),b=z.sig(20);
        const query=z.sig(()=>{if(!mode.$)return a.$;b.$;throw new Error('intentional_query_failure');});
        assert.equal(query.$,10);let oldEdges=0;for(let edge=query.dh;edge;edge=edge.nd)oldEdges++;
        let failed=0;const failStart=performance.now();
        for(let i=0;i<edits;i++){mode.$=true;try{query.$;}catch(e){assert.equal(e.message,'intentional_query_failure');failed++;}mode.$=false;assert.equal(query.$,10);}
        const failureRecoveryMs=performance.now()-failStart;assert.equal(failed,edits);
        let finalEdges=0;for(let edge=query.dh;edge;edge=edge.nd)finalEdges++;
        console.log(JSON.stringify({arm,nodes:signals.length,edges:graph.nodes.reduce((n,x)=>n+x.deps.length,0),edits,coldMs,editMs,editMeanMs:editMs/edits,initialRuns:beforeRuns,editRuns,checksum,initialStats:fixed?initialStats:undefined,editStats,failureRecoveryMs,failureAttempts:failed,failureControl:'Different failed-dependency contracts; absolute costs only, not a comparable speed ratio',oldEdges,finalEdges,allStats:z.adapterStats}));
    } finally {await fs.rm(directory,{recursive:true,force:true});}
} else {
    const [input,graphDirectory,outputArg,blocksArg='6',editsArg='100',...extra]=args;
    if(!input||!graphDirectory||!outputArg||extra.length)throw new Error('Usage: node zit_bench.mjs original.ts graph-directory output.json [blocks=6] [edits=100]');
    const blocks=Number(blocksArg),edits=Number(editsArg);assert.ok(blocks>0&&blocks%2===0&&edits>=2);
    const cases=['large-128','large-512','large-2048','large-8192','large-skew-8','even-64x64','body-16x16'];
    const rows=[],graphs=[];
    for(const name of cases) {
        const graphPath=path.resolve(graphDirectory,'callgraph-'+name+'.json'),graphBytes=await fs.readFile(graphPath);
        graphs.push({case:name,path:graphPath,sha256:sha(graphBytes),provenance:JSON.parse(graphBytes).provenance});
        for(let block=0;block<blocks;block++)for(const arm of block%2?['adapted','original']:['original','adapted']) {
            const started=performance.now();
            const result=JSON.parse(execFileSync(process.execPath,['--expose-gc',fileURLToPath(import.meta.url),'--worker',path.resolve(input),graphPath,arm,String(edits)],{encoding:'utf8',stdio:['ignore','pipe','pipe'],maxBuffer:8*1024*1024}));
            rows.push({case:name,block,...result,workerWallMs:performance.now()-started});
        }
    }
    const summary=cases.map(name=>{
        const data=rows.filter(r=>r.case===name),old=data.filter(r=>r.arm==='original'),fixed=data.filter(r=>r.arm==='adapted');
        return {case:name,originalColdMs:median(old.map(r=>r.coldMs)),adaptedColdMs:median(fixed.map(r=>r.coldMs)),originalEditMeanMs:median(old.map(r=>r.editMeanMs)),adaptedEditMeanMs:median(fixed.map(r=>r.editMeanMs)),pairedColdRatio:median(fixed.map(r=>r.coldMs/old.find(x=>x.block===r.block).coldMs)),pairedEditRatio:median(fixed.map(r=>r.editMs/old.find(x=>x.block===r.block).editMs)),originalFailureRecoveryMs:median(old.map(r=>r.failureRecoveryMs)),adaptedFailureRecoveryMs:median(fixed.map(r=>r.failureRecoveryMs))};
    });
    const result={scope:'Original versus adapted synchronous numeric zit kernel on preserved actual compiler direct-call topology. Not semantic compilation or CLI speed. Each arm has a fresh Node process; erasure/import/startup excluded from coldMs but included in workerWallMs. Per-query failed-dependency contracts differ; failure costs have no speed ratio.',blocks,edits,node:process.version,source_sha256:sha(await fs.readFile(input)),adapter_sha256:sha(await fs.readFile(new URL('./zit_adapter.mjs',import.meta.url))),benchmark_sha256:sha(await fs.readFile(new URL('./zit_bench.mjs',import.meta.url))),graphs,rows,summary};
    const output=path.resolve(outputArg);await fs.mkdir(path.dirname(output),{recursive:true});await fs.writeFile(output,JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(summary,null,2));
}
