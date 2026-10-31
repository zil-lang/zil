// Research adapter for the supplied zit synchronous numeric kernel only.
// Preserve the source file and make every replaced region exact and reviewable.
export const prelude = `
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

const support = `
const adapterNodes=new Set(), adapterActive=new Set();let adapterEpoch=0;
export const adapterStats={reads:0, invalidationNodes:0, invalidationEdges:0, dependencyValidations:0, dependencyEdgesValidated:0, snapshotEdges:0, shapeEdges:0, consumerEdgesTraversed:0, repairs:0, stableRepairsSkipped:0, nodesScanned:0, edgesScanned:0, queueRebuckets:0, cyclesRejected:0, rollbacks:0};
// Mark only possibly affected consumers; preserve the original direct dirty
// notification and equality barrier. This avoids validating unrelated leaves
// merely because some other source in the process changed.
function adapterInvalidate(source) {
    adapterEpoch++;
    const seen=new Set([source]),pending=[source];
    for(let i=0;i<pending.length;i++)for(let edge=pending[i].wh;edge;edge=edge.nw) {
        adapterStats.invalidationEdges++;
        if(!seen.has(edge.w)) {seen.add(edge.w);pending.push(edge.w);edge.w.adapterMaybeEpoch=adapterEpoch;adapterStats.invalidationNodes++;}
    }
}
// Current dependencies are a transaction: stale suffix edges of an active
// consumer do not constrain the candidate graph. Recompute only the changed
// node's current transitive consumers, in an iterative exact topological pass.
function adapterRepair(changed, before) {
    const after=[];for(let e=changed.dh;e;e=e.nd){adapterStats.shapeEdges++;after.push(e.d);}
    const wantedRank=after.reduce((rank,d)=>(adapterStats.shapeEdges++,Math.max(rank,(d.h||0)+1)),1);
    if(changed.h===wantedRank&&before.length===after.length&&before.every((pair,i)=>(adapterStats.shapeEdges++,pair[0].d===after[i]))) {
        adapterStats.stableRepairsSkipped++;return;
    }
    adapterStats.repairs++;
    const affected=new Set([changed]),nodes=[changed];
    for(let i=0;i<nodes.length;i++) for(let e=nodes[i].wh;e;e=e.nw) {
        adapterStats.consumerEdgesTraversed++;
        if(!(e.w.t&4)||adapterActive.has(e.w)&&e.vn!==e.w.vn)continue;
        if(!affected.has(e.w)) {affected.add(e.w);nodes.push(e.w);}
    }
    const indegree=new Map(), consumers=new Map(), rank=new Map();
    for(const n of nodes) {indegree.set(n,0);consumers.set(n,[]);rank.set(n,1);}
    for(const n of nodes) {
        adapterStats.nodesScanned++;
        for(let e=n.dh;e;e=e.nd) {
            if(adapterActive.has(n)&&e.vn!==n.vn)continue;
            adapterStats.edgesScanned++;
            if(affected.has(e.d)) {indegree.set(n,indegree.get(n)+1);consumers.get(e.d).push(n);}
            else rank.set(n,Math.max(rank.get(n),(e.d.h||0)+1));
        }
    }
    const ready=nodes.filter(n=>!indegree.get(n));let i=0;
    for(;i<ready.length;i++) for(const n of consumers.get(ready[i])) {
        rank.set(n,Math.max(rank.get(n),rank.get(ready[i])+1));
        const count=indegree.get(n)-1;indegree.set(n,count);if(!count)ready.push(n);
    }
    if(i!==nodes.length) {adapterStats.cyclesRejected++;throw new Error('zit_adapter_exact_cycle');}
    let queuedHeightChanged=false;
    for(const n of nodes) {if(n.nq&&n.h!==rank.get(n))queuedHeightChanged=true;n.h=rank.get(n);}
    if(!queuedHeightChanged)return;
    // A downward rank change must also move already queued computations.
    const queued=[];
    for(let h=0;h<HQ.length;h++) for(let w=HQ[h];w;) {const next=w.nq===w?undefined:w.nq;w.nq=undefined;queued.push(w);w=next;}
    HQ.length=HT.length=0;$h=Infinity;
    for(const w of queued) {adapterStats.queueRebuckets++;hq(w);}
}
function adapterRestore(w, before) {
    bdClr(w); w.dt=undefined;
    for(const [b,vn] of before) {
        b.vn=vn;b.pd=w.dt;b.nd=undefined;
        if(w.dt)w.dt.nd=b;else w.dh=b;w.dt=b;
        b.pw=b.d.wt;b.nw=undefined;
        if(b.pw)b.pw.nw=b;else b.d.wh=b;b.d.wt=b;
    }
}
export function adapterGraph() {
    return [...adapterNodes].filter(n=>n.t&4).map(n=>({node:n,rank:n.h,deps:(()=>{let a=[];for(let e=n.dh;e;e=e.nd)a.push(e.d);return a;})()}));
}
`;

const read = `function read(s=this) {
    adapterStats.reads++;
    if(adapterActive.has(s)) {adapterStats.cyclesRejected++;throw new Error('zit_adapter_exact_cycle');}
    if(s.t&4) {
        if(!s.dty&&!s.adapterError&&(s.adapterMaybeEpoch||0)>(s.adapterChecked??-1)) {
            adapterStats.dependencyValidations++;const oldBond=$b;$b=0;
            try {for(let e=s.dh;e;e=e.nd){adapterStats.dependencyEdgesValidated++;read(e.d);}} finally {$b=oldBond;}
        }
        (s.dty||s.adapterError)&&recalc(s,1);
        if(s.adapterError)throw s.adapterError;
        s.adapterChecked=adapterEpoch;
    }
    if(s.st&APE)throw s;
    $b&&bond(s);return s._v;
}`;

const recalc = `let recalc=(w,up)=>{
    const before=[];for(let e=w.dh;e;e=e.nd){adapterStats.snapshotEdges++;before.push([e,e.vn]);}
    const pb=$b,pw=$w,oldVn=w.vn,oldValue=w._v,oldRank=w.h;
    w.dty=undefined;$g++;w.dt=undefined;w.vn++;adapterActive.add(w);
    try {
        $w=w;$b=w.t;const value=w.f(w);
        if(tyAsy(value))throw new Error('zit_adapter_sync_only');
        bdTrim(w);adapterRepair(w,before);w.adapterError=undefined;w.adapterChecked=adapterEpoch;isSig(w)&&write(w,value);
    } catch(error) {
        adapterStats.rollbacks++;adapterRestore(w,before);w.vn=oldVn;w._v=oldValue;w.h=oldRank;w.adapterError=error;
        if(up)throw error;onErr(error,w);
    } finally {$b=pb;$w=pw;adapterActive.delete(w);}
};`;

export function adapt(original) {
    let source=original.replaceAll('\r\n','\n').split('\n').filter(line=>!line.startsWith('import ')).join('\n');
    const replace=(from,to)=>{if(source.split(from).length!==2)throw new Error('Source region missing/ambiguous: '+from.slice(0,70));source=source.replace(from,to);};
    replace('this._v=v; sys||$hk.sig?.(this);','this._v=v; adapterNodes.add(this); sys||$hk.sig?.(this);');
    replace('if(o===v&&!op) return 0; s.h||$ops?.push','if(o===v&&!op) return 0; if(!s.h)adapterInvalidate(s); s.h||$ops?.push');
    const start=source.indexOf('function read(s=this)'),end=source.indexOf('\n\n// write：',start);
    if(start<0||end<0)throw new Error('read boundary missing');source=source.slice(0,start)+read+source.slice(end);
    const a=source.indexOf('let recalc=(w: C,up?) => {'),b=source.indexOf('\n/** 勿删：另一类环检测',a);
    if(a<0||b<0)throw new Error('recalc boundary missing');source=source.slice(0,a)+recalc+source.slice(b);
    replace('if(w.h<d.h+1) w.h=d.h+1,w.wh&&hUp(w,w.h+1);','');
    const h=source.indexOf('let hUp='),he=source.indexOf('\nlet unbond=',h);
    if(h<0||he<0)throw new Error('hUp boundary missing');source=source.slice(0,h)+source.slice(he);
    return prelude+support+source;
}
