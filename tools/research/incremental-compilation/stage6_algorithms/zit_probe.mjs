import fs from 'node:fs/promises';
import {stripTypeScriptTypes} from 'node:module';
import {createHash} from 'node:crypto';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
import {adapt,prelude} from './zit_adapter.mjs';

const [inputArg,outputArg,graphArg,...extra]=process.argv.slice(2);
if(!inputArg||!outputArg||extra.length)throw new Error('Usage: node zit_probe.mjs original-zit.ts result.json [call-graph.json]');
const input=path.resolve(inputArg),output=path.resolve(outputArg),bytes=await fs.readFile(input),original=bytes.toString('utf8');
const sha=b=>createHash('sha256').update(b).digest('hex');
assert.equal(sha(bytes),'719a914356a24ac1edbeb1ffe6637d22311c9299e6be9ddbcdf3f7fdddf1474f');
const temporary=await fs.mkdtemp(path.join(tmpdir(),'zil-zit-adapter-'));let serial=0;
async function load(fixed=true) {
    const transformed=fixed?adapt(original):prelude+original.split('\n').filter(l=>!l.startsWith('import ')).join('\n');
    const file=path.join(temporary,'kernel-'+serial+++'.mjs');
    await fs.writeFile(file,stripTypeScriptTypes(transformed,{mode:'transform'}));
    const z=await import(pathToFileURL(file));const errors=[];z.$hk.err=e=>errors.push(e.message);return {z,errors};
}
const edges=c=>{let a=[];for(let e=c.dh;e;e=e.nd)a.push(e);return a;};
const rows=[];
try {
    for(const fixed of [false,true]) {
        const {z,errors}=await load(fixed);const flag=z.sig(false);let a,b,runs=0;
        a=z.sig(()=>flag.$?b.$+1:1);b=z.sig(()=>flag.$?1:a.$+1);
        const sink=z.sig(()=>{runs++;return a.$+b.$;});assert.equal(sink.$,3);
        const changes=fixed?5000:1100;
        for(let i=0;i<changes;i++) {flag.$=!flag.$;z.flush();assert.equal(sink.$,3);}
        if(fixed){assert.equal(errors.length,0);assert.deepEqual([a.h,b.h,sink.h],[1,2,3]);}
        rows.push({case:'acyclic_direction_changes',fixed,changes,finalRank:[a.h,b.h,sink.h],warnings:errors.length,sinkRuns:runs,stats:z.adapterStats});
    }
    {
        const {z,errors}=await load();const input=z.sig(0);let middleRuns=0,leftRuns=0,rightRuns=0,sinkRuns=0;
        const middle=z.sig(()=>{middleRuns++;return input.$%2;}),left=z.sig(()=>{leftRuns++;return middle.$+1;}),right=z.sig(()=>{rightRuns++;return middle.$+2;});
        const sink=z.sig(()=>{sinkRuns++;return left.$+right.$;});assert.equal(sink.$,3);
        input.$=2;input.$=4;z.flush();assert.equal(sink.$,3);assert.deepEqual([middleRuns,leftRuns,rightRuns,sinkRuns],[2,1,1,1]);
        input.$=5;input.$=7;z.flush();assert.equal(sink.$,5);assert.deepEqual([middleRuns,leftRuns,rightRuns,sinkRuns],[3,2,2,2]);
        assert.equal(errors.length,0);rows.push({case:'batched_diamond_and_equal_barrier',middleRuns,leftRuns,rightRuns,sinkRuns,stats:z.adapterStats});
    }
    {
        const {z,errors}=await load();const mode=z.sig(0),a=z.sig(10),b=z.sig(20);let runs=0;
        const c=z.sig(()=>{runs++;return mode.$===0?a.$+b.$:mode.$===1?b.$+a.$:a.$;});
        assert.equal(c.$,30);const first=edges(c);a.$=11;z.flush();assert.ok(first.every((e,i)=>e===edges(c)[i]));
        mode.$=1;z.flush();assert.equal(c.$,31);assert.equal(edges(c)[0],first[0]);assert.equal(edges(c)[2],first[1]);
        mode.$=2;z.flush();const before=runs;b.$=99;z.flush();assert.equal(runs,before);assert.equal(c.$,11);assert.equal(edges(c).length,2);
        assert.equal(errors.length,0);rows.push({case:'stable_bonds_reorder_and_trim',stableEdgesReused:3,removedProducerNoLongerTriggers:true,runs,stats:z.adapterStats});
    }
    {
        const {z}=await load();let a,b;a=z.sig(()=>b.$+1);b=z.sig(()=>a.$+1);
        assert.throws(()=>a.$,/zit_adapter_exact_cycle/);assert.equal(edges(a).length,0);assert.equal(edges(b).length,0);
        rows.push({case:'true_recursive_read_cycle',rejected:true,failedEdgesRolledBack:true,stats:z.adapterStats});
    }
    {
        const {z}=await load();const mode=z.sig(false);let b;const a=z.sig(()=>mode.$?b.$+1:1);b=z.sig(()=>a.$+1);
        assert.equal(b.$,2);mode.$=true;assert.throws(()=>a.$,/zit_adapter_exact_cycle/);
        assert.equal(a._v,1);assert.deepEqual(edges(a).map(e=>e.d),[mode]);mode.$=false;z.flush();assert.equal(b.$,2);
        rows.push({case:'cycle_through_clean_cached_dependency',rejected:true,recovered:true,stats:z.adapterStats});
    }
    {
        const {z}=await load();const mode=z.sig(0),a=z.sig(10),b=z.sig(20);let runs=0;
        const c=z.sig(()=>{runs++;const m=mode.$;if(m===0)return a.$;const value=b.$;if(m===1)throw new Error('query_failure');return value;});
        assert.equal(c.$,10);const before=edges(c);mode.$=1;z.flush();assert.throws(()=>c.$,/query_failure/);assert.equal(c._v,10);
        assert.deepEqual(edges(c),before);assert.deepEqual(edges(c).map(e=>e.d),[mode,a]);
        b.$=40;assert.throws(()=>c.$,/query_failure/);mode.$=2;z.flush();assert.equal(c.$,40);
        const after=runs;a.$=99;z.flush();assert.equal(runs,after);
        rows.push({case:'failed_query_dependency_and_value_transaction',oldEdgesAndValuePreserved:true,failedQueriesNeverGreen:true,recoveredValue:c.$,stats:z.adapterStats});
    }
    {
        const {z,errors}=await load();const source=z.sig(0);let last=source;const depth=1200;
        for(let i=0;i<depth;i++){const p=last;last=z.sig(()=>p.$+1);assert.equal(last.$,i+1);}
        source.$=1;z.flush();assert.equal(last.$,depth+1);assert.equal(last.h,depth);assert.equal(errors.length,0);
        rows.push({case:'valid_chain_above_old_height_threshold',depth,finalValue:last.$,maxRank:last.h,stats:z.adapterStats});
    }
    {
        const {z,errors}=await load();const version=z.sig(0),nodes=[],choices=[],runs=[];
        let state=0x31415926;const random=()=>{state=(Math.imul(state,1664525)+1013904223)>>>0;return state;};
        const width=40,changes=500;
        for(let i=0;i<width;i++){choices[i]=[];runs[i]=0;nodes[i]=z.sig(()=>{runs[i]++;version.$;let v=i+1;for(const d of choices[i])v=(v+nodes[d].$)>>>0;return v;});nodes[i].$;}
        for(let step=0;step<changes;step++) {
            for(let i=1;i<width;i++){const selected=new Set();for(let j=0,count=random()%5;j<count;j++)selected.add(random()%i);choices[i]=[...selected];}
            version.$=step+1;z.flush();const values=[];
            for(let i=0;i<width;i++){let v=i+1;for(const d of choices[i])v=(v+values[d])>>>0;values[i]=v;assert.equal(nodes[i].$,v);}
            for(const row of z.adapterGraph()){let rank=1;for(const d of row.deps)rank=Math.max(rank,(d.h||0)+1);assert.equal(row.rank,rank);}
        }
        assert.equal(errors.length,0);rows.push({case:'random_dynamic_dag_against_fresh_topological_oracle',width,changes,valueComparisons:width*changes,stats:z.adapterStats});
    }
    if(graphArg) {
        // Input order must be producer-first and acyclic. Real compiler call
        // graphs containing recursion need SCC condensation by the exporter.
        const graphBytes=await fs.readFile(graphArg),graph=JSON.parse(graphBytes);
        for(const fixed of [false,true]) {
        const {z,errors}=await load(fixed);
        const signals=[],runs=Array(graph.nodes.length).fill(0),inputs=graph.nodes.map(()=>z.sig(0));
        for(let i=0;i<graph.nodes.length;i++) {
            const n=graph.nodes[i];assert.ok(n.deps.every(d=>Number.isInteger(d)&&d>=0&&d<i));
            signals.push(z.sig(()=>{runs[i]++;let v=inputs[i].$;for(const d of n.deps)v=(v+signals[d].$)>>>0;return v;}));signals[i].$;
        }
        const initial=runs.reduce((a,b)=>a+b,0),edit=graph.edit_node??0,beforeStats={...z.adapterStats};inputs[edit].$=1;z.flush();
        const editStats=z.adapterStats?Object.fromEntries(Object.entries(z.adapterStats).map(([k,v])=>[k,v-beforeStats[k]])):undefined;
        for(let i=0;i<signals.length;i++){let v=inputs[i].$;for(const d of graph.nodes[i].deps)v=(v+signals[d].$)>>>0;assert.equal(signals[i].$,v);}
        assert.equal(errors.length,0);rows.push({case:'exported_compiler_dependency_topology',fixed,graph_sha256:sha(graphBytes),provenance:graph.provenance,nodes:signals.length,edges:graph.nodes.reduce((s,n)=>s+n.deps.length,0),editNode:edit,initialRuns:initial,editRuns:runs.reduce((a,b)=>a+b,0)-initial,wholeGraphRecomputeRuns:signals.length,editStats,stats:z.adapterStats,scope:'Scalar propagation over actual graph topology; not compiler execution or compile timing'});
        }
    }
    const result={source:input,source_sha256:sha(bytes),adapter_sha256:sha(await fs.readFile(new URL('./zit_adapter.mjs',import.meta.url))),probe_sha256:sha(await fs.readFile(new URL('./zit_probe.mjs',import.meta.url))),node:process.version,scope:'Original synchronous numeric zit kernel adapted for stable query nodes; no DOM/async/child-owner support, compiler integration or timing claim. Affected-consumer Kahn repair cost counted in full; stable dependencies skip rank repair. Per-query rollback only; nested-query commits are not a compiler-wide atomic transaction.',timing:'NotRun; parent coordinates exclusive performance windows',rows};
    await fs.mkdir(path.dirname(output),{recursive:true});await fs.writeFile(output,JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(rows,null,2));
} finally {await fs.rm(temporary,{recursive:true,force:true});}
