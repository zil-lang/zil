/* Standalone synchronous scalar query-graph research, NOT the ZIL compiler.
 * No imports; runs in Node or a browser console. Public API: ZilReactiveProbe.
 * Stable owners, versioned inputs, reused bidirectional bonds, dynamic reads,
 * equality barriers, atomic input batches, height buckets with queue dedup.
 * The independent full evaluator is the correctness oracle, not a speed claim.
 */
(() => {
'use strict';
const assert = (ok, msg = 'assertion failed') => { if (!ok) throw Error(msg); };
const equal = (a,b) => JSON.stringify(a) === JSON.stringify(b);
const stats = () => ({runs:0, reads:0, validations:0, notifications:0, enqueues:0,
  dedup:0, reusedBonds:0, newBonds:0, removedBonds:0, rankVisits:0});
function test(tx,n,c=1,f) {f? (c*=n):(f=c,c=n); let p=performance,t=p.now(),i=0; for(;i<n;i++) f(); t=p.now()-t; console.log(tx.padEnd(30)+t.toFixed(3).padStart(12)+`ms,约${(c/t|0).toLocaleString().padStart(12)} 次/ms`);}
class Heap {
  constructor(){this.a=[];}
  push(x){let a=this.a,i=a.length; a.push(x); while(i){let p=(i-1)>>1;if(a[p]<=x)break;a[i]=a[p];i=p;}a[i]=x;}
  pop(){let a=this.a,r=a[0],x=a.pop();if(a.length){let i=0;while(i*2+1<a.length){let c=i*2+1;if(c+1<a.length&&a[c+1]<a[c])c++;if(a[c]>=x)break;a[i]=a[c];i=c;}a[i]=x;}return r;}
}
class Graph {
  constructor(spec,mode='height') {
    assert(['height','pull','invalidate'].includes(mode),'unknown mode');
    this.spec=spec;this.mode=mode;this.epoch=0;this.busy=false;this.s=stats();
    this.nodes=spec.nodes.map((d,id)=>({id,fn:d.fn,value:d.value,init:!d.fn,rev:0,
      deps:new Map(),users:new Set(),h:0,dirty:!!d.fn,active:false,stamp:-1,ticket:0,queued:false}));
    this.clearQueue();this.cold();
  }
  clearQueue(){this.buckets=new Map();this.heap=new Heap();}
  cold(){
    this.coldMode=true;
    try {for(const id of this.spec.roots)this.ensure(this.nodes[id]);}
    finally {this.coldMode=false;}
    this.clearQueue();for(const n of this.nodes)n.queued=false;
  }
  enqueue(n){
    if(n.queued){this.s.dedup++;return;}
    n.queued=true;const item=[n,++n.ticket];
    let b=this.buckets.get(n.h);if(!b){b=[];this.buckets.set(n.h,b);this.heap.push(n.h);}
    b.push(item);this.s.enqueues++;
  }
  mark(n){
    if(n.active)return;
    if(n.dirty){this.s.dedup++;return;}
    n.dirty=true;n.stamp=-1;
    if(this.mode==='height')this.enqueue(n);
  }
  notify(n){
    if(this.coldMode||this.mode==='pull')return;
    if(this.mode==='invalidate'){
      const q=[n];for(let i=0;i<q.length;i++)for(const b of q[i].users){
        this.s.notifications++;const w=b.to;if(!w.dirty&&!w.active){this.mark(w);q.push(w);}else this.s.dedup++;
      }
    }else for(const b of n.users){this.s.notifications++;this.mark(b.to);}
  }
  repair(start){
    if(this.mode!=='height')return;
    const q=[start],inQ=new Set(q);
    for(let i=0;i<q.length;i++){
      const n=q[i];inQ.delete(n);this.s.rankVisits++;
      let h=0;for(const b of n.deps.values())h=Math.max(h,b.from.h+1);
      if(n.h===h)continue;n.h=h;
      if(n.queued){n.queued=false;this.enqueue(n);}
      for(const b of n.users)if(!inQ.has(b.to)){inQ.add(b.to);q.push(b.to);}
      assert(i<10_000_000,'rank repair budget exceeded');
    }
  }
  ensure(n){
    assert(!n.active,'query cycle');
    if(!n.fn)return n.value;
    if(n.stamp===this.epoch&&!n.dirty)return n.value;
    if(n.dirty||!n.init)return this.evaluate(n,true);
    // Validate old dependencies only on pull/new dynamic reads, never as a
    // compulsory whole-graph scan before a height-scheduled transaction.
    n.active=true;
    try {for(const b of n.deps.values()){
      this.s.validations++;this.ensure(b.from);
      if(b.seen!==b.from.rev){n.dirty=true;break;}
    }}finally{n.active=false;}
    if(n.dirty)return this.evaluate(n,true);
    n.stamp=this.epoch;return n.value;
  }
  evaluate(n,validate=false){
    assert(!n.active,'query cycle');n.active=true;this.s.runs++;
    const next=new Map();let v;
    try {v=n.fn(id=>{
      const d=this.nodes[id];assert(d,'unknown dependency');this.s.reads++;
      assert(!d.active,'query cycle');
      const old=n.deps.get(id);
      // New dependencies must be current even when their old rank is above
      // the running node. Pull that ancestry, not unrelated global buckets.
      const value=this.mode==='invalidate'?((d.dirty||!d.init)?this.evaluate(d):d.value):
        (validate||!old||this.mode==='pull')?this.ensure(d):
        d.dirty?this.evaluate(d,true):d.value;
      if(!next.has(id))next.set(id,{bond:old||{from:d,to:n,seen:d.rev},seen:d.rev});
      return value;
    });}finally{n.active=false;}
    let structural=false;
    for(const [id,b] of n.deps)if(!next.has(id)){structural=true;if(this.mode!=='pull')b.from.users.delete(b);this.s.removedBonds++;}
    const committed=new Map();
    for(const [id,{bond,seen}] of next){
      if(n.deps.has(id))this.s.reusedBonds++;else{structural=true;if(this.mode!=='pull')bond.from.users.add(bond);this.s.newBonds++;}
      bond.seen=seen;committed.set(id,bond);
    }
    n.deps=committed;n.dirty=false;n.stamp=this.epoch;
    if(n.queued){n.queued=false;n.ticket++;}
    if(structural)this.repair(n);
    const changed=!n.init||!Object.is(v,n.value);n.value=v;n.init=true;
    if(changed){n.rev++;this.notify(n);}return v;
  }
  flush(){
    if(this.mode==='pull'){for(const id of this.spec.roots)this.ensure(this.nodes[id]);return;}
    if(this.mode==='invalidate'){for(const id of this.spec.roots){const n=this.nodes[id];if(n.dirty||!n.init)this.evaluate(n);}return;}
    while(this.heap.a.length){
      const h=this.heap.pop(),b=this.buckets.get(h);this.buckets.delete(h);
      for(const [n,t] of b){
        if(!n.queued||n.ticket!==t)continue;
        n.queued=false;if(n.dirty)this.evaluate(n);
      }
    }
  }
  resetAfterFailure(){
    for(const n of this.nodes)if(n.fn){n.value=undefined;n.init=false;n.rev=0;n.h=0;n.dirty=true;n.active=false;n.stamp=-1;n.queued=false;n.ticket++;n.deps.clear();}
    for(const n of this.nodes)n.users.clear();
    this.clearQueue();this.cold();
  }
  batch(changes,{abort=false}={}){
    assert(!this.busy,'nested batches unsupported');
    const staged=new Map(changes),undo=[];
    for(const [id] of staged)assert(this.nodes[id]&&!this.nodes[id].fn,'input write required');
    if(abort)return this.output();
    this.busy=true;this.epoch++;this.s=stats();
    try{
      for(const [id,value] of staged){const n=this.nodes[id];if(Object.is(value,n.value))continue;undo.push([n,n.value,n.rev]);n.value=value;n.rev++;}
      // All source values are installed before any consumer executes.
      if(!undo.length)return this.output();
      for(const [n] of undo)this.notify(n);
      this.flush();return this.output();
    }catch(e){
      for(const [n,value,rev] of undo){n.value=value;n.rev=rev;}
      this.epoch++;this.resetAfterFailure();throw e;
    }finally{this.busy=false;}
  }
  output(){return this.spec.roots.map(id=>this.nodes[id].value);}
  inspect(){
    let edges=0;for(const n of this.nodes){assert(!n.active,'active node leaked');
      for(const b of n.deps.values()){edges++;assert((this.mode==='pull'||b.from.users.has(b))&&b.to===n,'broken bond');if(this.mode==='height')assert(n.h>b.from.h,'bad rank');}
      for(const b of n.users)assert(b.to.deps.get(n.id)===b,'orphan reverse edge');
    }return{nodes:this.nodes.length,edges};
  }
}
function builder(){const nodes=[];return {nodes,roots:[],input(value){nodes.push({value});return nodes.length-1;},query(fn){nodes.push({fn});return nodes.length-1;}};}
// Independent per-request recursive interpreter: no persistent graph, bonds,
// heights, versions, equality barriers or invalidation from the candidate.
function oracle(spec,values){const memo=new Map(),active=new Set();
  const get=id=>{if(memo.has(id))return memo.get(id);assert(!active.has(id),'oracle cycle');active.add(id);
    const d=spec.nodes[id],v=d.fn?d.fn(get):values.get(id);active.delete(id);memo.set(id,v);return v;};
  return spec.roots.map(get);
}
function inputs(spec){return new Map(spec.nodes.flatMap((n,i)=>n.fn?[]:[[i,n.value]]));}
function rng(seed){return()=>{seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;return seed>>>0;};}
function median(xs){xs=[...xs].sort((a,b)=>a-b);return xs[xs.length>>1];}
// Compiler-shaped scalar fixture, NOT ZIL Source/Types/Verify. String inputs
// represent source records; declaration/body projections are exact scalars.
function program(n){
  const b=builder(),src=[],decl=[],body=[],types=[],code=[];
  const config=b.input(0),namespace=b.input(1),alternate=b.input(19);
  for(let i=0;i<n;i++){
    const s=b.input(`1:${i%97}:comment0`);src.push(s);
    const parsed=b.query(g=>g(s).split(':').slice(0,2).join(':'));
    decl.push(b.query(g=>+g(parsed).split(':')[0]));
    body.push(b.query(g=>+g(parsed).split(':')[1]));
  }
  for(let i=0;i<n;i++){
    types.push(b.query(g=>(g(body[i])+g(decl[(i+1)%n])+g(namespace))|0));
    const effect=b.query(g=>g(types[i])&3);
    code.push(b.query(g=>(Math.imul(g(types[i]),31)+g(effect)+(g(config)?g(alternate):0))|0));
  }
  const groups=[];for(let i=0;i<n;i+=64){const ids=code.slice(i,i+64);groups.push(b.query(g=>{let v=0;for(const id of ids)v=(v+g(id))|0;return v;}));}
  b.roots=[b.query(g=>{let v=0;for(const id of groups)v=(v+g(id))|0;return v;})];
  return {spec:b,src,decl,body,code,config,namespace,alternate};
}
function diamond(width){const b=builder(),x=b.input(1),y=b.input(2),items=[];
  for(let i=0;i<width;i++){const a=b.query(g=>g(x)+i),c=b.query(g=>g(y)-i);items.push(b.query(g=>g(a)+g(c)));}
  b.roots=[b.query(g=>items.reduce((s,id)=>s+g(id),0))];return{spec:b,x,y};
}
function controls(){
  let comparisons=0,batches=0;const modes=['height','pull','invalidate'];
  function compare(spec,edits){const gs=modes.map(m=>new Graph(spec,m)),v=inputs(spec);
    for(const edit of edits){for(const [id,value] of edit)v.set(id,value);const expected=oracle(spec,v);
      for(const g of gs){assert(equal(g.batch(edit),expected),`differential ${g.mode}`);g.inspect();comparisons++;}batches++;
    }return gs;
  }
  const d=diamond(128);const dg=compare(d.spec,[[[d.x,2],[d.y,3]],[[d.x,4],[d.x,5],[d.y,8]]]);
  assert(dg[0].s.runs===385,'height diamond must execute each query once');
  const b=builder(),s=b.input(1),p=b.query(g=>g(s)&1),tail=b.query(g=>g(p)*9);b.roots=[tail];
  const eq=compare(b,[[[s,3]]]);assert(eq[0].s.runs===1&&eq[2].s.runs===2,'equality barrier');
  const c=builder(),x=c.input(2);let last=x;for(let i=0;i<1200;i++){const prev=last;last=c.query(g=>g(prev)+1);}c.roots=[last];compare(c,[[[x,3]]]);
  // Dynamic edge reversal remains acyclic, though the previous graph has the
  // opposite order. A separate true cycle must fail and restore the old epoch.
  for(const mode of modes){const q=builder(),sel=q.input(0);let a,z;
    a=q.query(g=>g(sel)===0?1:g(z)+1);z=q.query(g=>g(sel)===0?g(a)+1:g(sel)===2?g(a)+1:1);q.roots=[a,z];
    const g=new Graph(q,mode);for(let i=0;i<200;i++){const v=i&1;assert(equal(g.batch([[sel,v]]),v?[2,1]:[1,2]),'dynamic order');g.inspect();comparisons++;batches++;}
    const old=g.output();assert(equal(g.batch([[sel,0]],{abort:true}),old),'abort');
    let failed=false;try{g.batch([[sel,2]]);}catch(e){failed=/cycle/.test(e.message);}assert(failed&&equal(g.output(),old),'cycle rollback');g.inspect();
  }
  const pr=program(48),v=inputs(pr.spec),gs=modes.map(m=>new Graph(pr.spec,m)),r=rng(0x514c);
  for(let step=0;step<500;step++){
    let edits=[];for(let k=0;k<1+(r()%8);k++){const j=r()%pr.src.length;edits.push([pr.src[j],`${1+r()%3}:${r()%997}:comment${step}`]);}
    if(step%7===0)edits.push([pr.config,r()%2]);if(step%11===0)edits.push([pr.namespace,r()%5]);if(step%13===0)edits.push([pr.alternate,r()%29]);
    for(const [id,value]of edits)v.set(id,value);const expected=oracle(pr.spec,v);
    for(const g of gs){assert(equal(g.batch(edits),expected),'random semantic graph');g.inspect();comparisons++;}batches++;
  }
  // Random changing DAG order: stable nodes, independently versioned edge lists.
  const dy=builder(),configs=[],values=[],queries=[],rr=rng(0x51a1);
  for(let i=0;i<32;i++){configs.push(dy.input(''));values.push(dy.input(i));}
  for(let i=0;i<32;i++)queries.push(dy.query(g=>{const ds=g(configs[i]);let v=g(values[i]);if(ds)for(const j of ds.split(','))v=(v+g(queries[+j]))|0;return v;}));
  dy.roots=[...queries];const dv=inputs(dy),dgs=modes.map(m=>new Graph(dy,m));
  for(let k=0;k<200;k++){
    const order=Array.from({length:32},(_,i)=>i);for(let i=31;i>0;i--){const j=rr()%(i+1);[order[i],order[j]]=[order[j],order[i]];}
    const edits=[];for(let i=0;i<32;i++){const ds=[];for(let j=0;j<i;j++)if(rr()%8===0)ds.push(order[j]);edits.push([configs[order[i]],ds.join(',')]);}
    edits.push([values[rr()%32],rr()%997]);for(const [id,v]of edits)dv.set(id,v);const out=oracle(dy,dv);
    for(const g of dgs){assert(equal(g.batch(edits),out),'random dynamic DAG');g.inspect();comparisons++;}batches++;
  }
  for(const mode of modes){const er=builder(),input=er.input(1),out=er.query(g=>{const v=g(input);if(v<0)throw Error('rejected input');return v*2;});er.roots=[out];const g=new Graph(er,mode);
    let failed=false;try{g.batch([[input,-1]]);}catch(e){failed=e.message==='rejected input';}assert(failed&&equal(g.output(),[2]),'error rollback');assert(equal(g.batch([[input,3]]),[6]),'error recovery');g.inspect();
  }
  // Namespace absence is an explicit input dependency, not an untracked miss.
  const n=builder(),present=n.input(false),av=n.input(7),bv=n.input(13),lookup=n.query(g=>g(present)?g(bv):g(av));n.roots=[lookup];
  compare(n,[[[bv,19]],[[present,true]],[[av,23]],[[present,false]]]);
  // Legal recursive analyses are represented as an SCC query whose local
  // monotone fixed point converges; call-graph recursion is not a query cycle.
  const f=builder(),seed=f.input(1),scc=f.query(g=>{let a=g(seed),b=2,again=true;while(again){const na=a|b,nb=b|a;again=na!==a||nb!==b;a=na;b=nb;}return a;}),use=f.query(g=>g(scc));f.roots=[use];compare(f,[[[seed,4]],[[seed,0]]]);
  return{passed:true,differentialComparisons:comparisons,inputBatches:batches,deepChain:1200,dynamicReversals:600,randomDynamicDags:200,
    scope:'synchronous scalar query model; SCC local solver fixture, not automatic SCC extraction'};
}
function bench({sizes=[512,4096],rounds=7}={}){
  const rows=[],modes=['height','pull','invalidate'];
  for(const n of sizes)for(const scenario of ['unchanged','private-one','private-64','comment-only','namespace-fanout','all-bodies','dynamic-switch']){
    const p=program(n),gs=new Map(),cold={};
    for(const mode of modes){const t=performance.now();gs.set(mode,new Graph(p.spec,mode));cold[mode]=performance.now()-t;}
    const samples=Object.fromEntries(modes.map(m=>[m,[]])),last={};
    for(let k=0;k<rounds+2;k++){
      const i=k+1,edits=scenario==='unchanged'?[]:scenario==='namespace-fanout'?[[p.namespace,i]]:scenario==='dynamic-switch'?[[p.config,i%2]]:
        Array.from({length:scenario==='all-bodies'?n:scenario==='private-64'?Math.min(n,64):1},(_,j)=>[p.src[j],scenario==='comment-only'?`1:${j%97}:comment${i}`:`1:${(j+i)%997}:comment0`]);
      let output;
      for(const mode of (k%2?[...modes].reverse():modes)){
        const g=gs.get(mode),t=performance.now(),v=g.batch(edits),ms=performance.now()-t;
        if(output)assert(equal(output,v),'benchmark output mismatch');else output=v;
        if(k>=2)samples[mode].push(ms);last[mode]={...g.s};
      }
    }
    rows.push({functions:n,scenario,graph:gs.get('height').inspect(),coldMs:cold,
      medianMs:Object.fromEntries(modes.map(m=>[m,median(samples[m])])),samplesMs:samples,work:last});
  }
  return{runtime:typeof process==='object'?process.version:'browser',rounds,rows,
    scope:'warm scalar model transactions only; no ZIL front end, LLVM, linker, compiler RSS or Rust comparison'};
}
const api={Graph,builder,oracle,inputs,program,diamond,controls,bench,test};globalThis.ZilReactiveProbe=api;
if(typeof process==='object'&&process.argv?.[1]?.endsWith('probe.js')){
  const result={controls:controls(),benchmark:bench({sizes:process.argv.includes('--large')?[512,4096,16384]:[512,4096],rounds:9})};
  console.log(JSON.stringify(result,null,2));
}
})();
