import fs from 'node:fs/promises';
import { stripTypeScriptTypes } from 'node:module';
import assert from 'node:assert/strict';

// Execute the uploaded reactive kernel verbatim after TypeScript erasure.
// Imported DOM/helper modules were not supplied. Only synchronous numeric
// reactive paths are exercised; the helpers used there are supplied below.
const input = '/workspace/scratch/4ab361722127/upload/zit.ts';
const original = await fs.readFile(input, 'utf8');
const prelude = `
const _=undefined, doc={}, Obj=Object, G=globalThis, ONCE={once:true};
const lisn=()=>{}, oHas=(o,k)=>Object.prototype.hasOwnProperty.call(o,k), oKeys=Object.keys;
const isFn=x=>typeof x==='function', isArr=Array.isArray, isObj=x=>x!==null&&typeof x==='object';
const isMap=x=>x instanceof Map, isStr=x=>typeof x==='string', isBsc=x=>x===null||typeof x!=='object'&&typeof x!=='function';
const raf=()=>{}, kebab=x=>x, Min=Math.min, Max=Math.max, nop=()=>{}, isIdx=x=>/^(0|[1-9][0-9]*)$/.test(x);
const isAsy=x=>!!x&&(typeof x.then==='function'||typeof x[Symbol.asyncIterator]==='function');
const tyAsy=x=>!x?0:typeof x[Symbol.asyncIterator]==='function'?2:typeof x.then==='function'?1:0;
const oDefs=Object.defineProperties, okr=x=>Promise.resolve(x), oAsg=Object.assign, oDes=Object.getOwnPropertyDescriptors;
const $ops=undefined, setMutReplay=()=>{}, $zss=undefined, $cls=new Set();
`;
const withoutImports = original.split('\n').filter(line=>!line.startsWith('import ')).join('\n');
const file='/tmp/zil-review/zit-mechanisms-kernel.mjs';
await fs.writeFile(file, stripTypeScriptTypes(prelude+withoutImports,{mode:'transform'}));
const z=await import(file);
const errors=[]; z.$hk.err=(e)=>errors.push(e.message);
const rows=[];

{
 let middleRuns=0,sinkRuns=0;
 const source=z.sig(0),middle=z.sig(()=>{middleRuns++;return source.$+1;});
 const sink=z.sig(()=>{sinkRuns++;return middle.$+1;});
 assert.equal(sink.$,2);source.$=1;
 const sinkInitiallyClean=!sink.dty;
 assert.ok(sinkInitiallyClean);assert.equal(sink.$,3);
 assert.deepEqual([middleRuns,sinkRuns],[2,2]);
 rows.push({case:'pull_read_flushes_dirty_ancestors',sinkInitiallyClean,middleRuns,sinkRuns,readReturnedCurrentValue:true});
}
{
 z.flush();
}
{
 let middleRuns=0, sinkRuns=0;
 const source=z.sig(0), middle=z.sig(()=>{middleRuns++;return source.$%2;});
 const sink=z.sig(()=>{sinkRuns++;return middle.$+1;});
 assert.equal(sink.$,1);
 source.$=2;
 assert.equal(middle.dty,1); assert.ok(!sink.dty);
 z.flush(); assert.equal(sink.$,1); assert.deepEqual([middleRuns,sinkRuns],[2,1]);
 rows.push({case:'point_to_point_equal_barrier',middleRuns,sinkRuns,directOnlyBeforeFlush:true});
}
{
 let leftRuns=0,rightRuns=0,sinkRuns=0;
 const source=z.sig(0), left=z.sig(()=>{leftRuns++;return source.$+1;});
 const right=z.sig(()=>{rightRuns++;return source.$+2;});
 const sink=z.sig(()=>{sinkRuns++;return left.$+right.$;});
 assert.equal(sink.$,3);
 source.$=1; source.$=2; z.flush();
 assert.equal(sink.$,7); assert.deepEqual([leftRuns,rightRuns,sinkRuns],[2,2,2]);
 rows.push({case:'batched_diamond_height_flush',leftRuns,rightRuns,sinkRuns,height:[left.h,right.h,sink.h]});
}
{
 const mode=z.sig(0),a=z.sig(10),b=z.sig(20);let runs=0;
 const c=z.sig(()=>{runs++;return mode.$===0?a.$+b.$:mode.$===1?b.$+a.$:a.$;});
 const edges=()=>{let out=[];for(let e=c.dh;e;e=e.nd)out.push(e);return out;};
 assert.equal(c.$,30);const first=edges();
 a.$=11;z.flush();const stable=edges();assert.ok(first.every((e,i)=>e===stable[i]));
 mode.$=1;z.flush();const reordered=edges();
 assert.equal(reordered[0],first[0]);assert.equal(reordered[2],first[1]);
 assert.notEqual(reordered[1],first[2]);
 mode.$=2;z.flush();assert.equal(edges().length,2);const before=runs;
 b.$=99;z.flush();assert.equal(runs,before);assert.equal(c.$,11);
 rows.push({case:'bond_cursor_reuse_and_trim',stableEdgesReused:first.length,reorderedPrefixReused:true,reorderedMovedEdgeRecreated:true,removedProducerNoLongerTriggers:true});
}
{
 const flag=z.sig(false);let a,b;
 a=z.sig(()=>flag.$?b.$+1:1);
 b=z.sig(()=>flag.$?1:a.$+1);
 const sink=z.sig(()=>a.$+b.$);
 assert.equal(sink.$,3);
 const startErrors=errors.length;
 for(let i=0;i<1100;i++){flag.$=!flag.$;z.flush();assert.equal(sink.$,3);}
 rows.push({case:'acyclic_dynamic_edge_direction_changes',changes:1100,finalHeight:[a.h,b.h,sink.h],cycleWarnings:errors.length-startErrors,eachCurrentGraphAcyclic:true});
}
await fs.writeFile('/tmp/zil-review/zit-mechanisms-result.json',JSON.stringify({input,scope:'Synchronous numeric paths from uploaded zit.ts with utility stubs; no DOM/asynchronous/performance claim',rows,errors},null,2));
console.log(JSON.stringify(rows,null,2));
