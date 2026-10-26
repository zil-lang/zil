import {test, expect} from 'bun:test';
import {mkdtempSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {bfs, fuzz, fuzzFsm, trials} from '../search';
import {createClock} from '../clock';
import {seedLoad} from '../seed-store';

test('fuzz stops at one failure by default and minimizes only when requested', async () => {
    const ctl={init:()=>0,ops:()=>({op:'tick'}),run:()=>{},chk:()=> 'failed'};
    const first=await fuzz(ctl,{n:5,steps:1});
    expect(first.n).toBe(1); expect(first.fail).toBe(1);
    expect(first.fails[0].min).toBeUndefined();
    const batch=await fuzz(ctl,{n:5,steps:1,maxFailures:3});
    expect(batch.n).toBe(3); expect(batch.fail).toBe(3);
});

test('roam-search-06 shrinking never exchanges same-prefix assertions or different lifecycle failures', async () => {
    const r=await fuzz({init:()=>({n:0}),ops:()=>({op:'tick'}),run:s=>{s.n++;},chk:s=>s.n?'bad 1 target':0,done:s=>s.n?0:'bad 2 unrelated'}, {n:1,steps:1,minimize:true});
    expect(r.fails[0].min).toEqual([{op:'tick'}]);
    const sameText=await fuzz({init:()=>({n:0}),ops:()=>({op:'tick'}),run:s=>{s.n++;},chk:s=>s.n?'same error':0,done:s=>s.n?0:'same error'}, {n:1,steps:1,minimize:true});
    expect(sameText.fails[0].min).toEqual([{op:'tick'}]);
});
test('roam-search-07 every thrown falsy value is a failure in every lifecycle phase', async () => {
    for(const value of [undefined,null,false,0,'',NaN]) for(const phase of ['init','ops','run','chk','done','drop']) {
        const ctl:any={init:()=>({}),ops:()=>({op:'tick'}),run:()=>{},chk:()=>0};
        ctl[phase]=()=>{throw value;};
        const r=await fuzz(ctl,{n:1,steps:1,minimize: true});
        expect(r.fail).toBe(1); expect(r.fails[0].why).toBe(value);
    }
});
test('roam-search-08 cleanup cannot overwrite an original falsy thrown value', async () => {
    const r=await fuzz({init:()=>0,ops:()=>({op:'tick'}),run:()=>{throw false;},chk:()=>0,drop:()=>{throw 0;}},{n:1,steps:1,minimize:true});
    expect(r.fail).toBe(1); expect(r.fails[0].why).toBe(false); expect(r.fails[0].cleanup).toEqual([0]);
    expect(r.fails[0].min).toEqual([{op:'tick'}]);
});

test('roam-search-01 partial initialization always cleans up and preserves both failures', async () => {
    let drops=0;
    let spec={init: () => {throw new Error('model-init');},ops: () => [],next: () => 0,key: String};
    let drv={reset: () => {},fire: () => {},chk: () => 0,drop: () => {drops++; throw new Error('cleanup');}};
    let graph=await bfs(spec,drv);
    expect(drops).toBe(1);
    expect(graph.bad.map(x => x.why.message)).toEqual(['model-init','cleanup']);
    let random=await fuzzFsm(spec,drv,{n: 1,minimize: false});
    expect(drops).toBe(2);
    expect(random.fails[0].why.message).toBe('model-init');
    expect(random.fails[0].cleanup?.[0].message).toBe('cleanup');
});

test('roam-search-02 terminal states run final checks without undefined actions', async () => {
    let checked=0;
    let r=await fuzzFsm({init: () => 0,ops: () => [],next: (m: number) => m,key: String}, {
        reset() {},fire() {throw new Error('unexpected action');},chk() {checked++; return 0;},
    },{n: 2});
    expect(r.fail).toBe(0); expect(r.n).toBe(2); expect(checked).toBe(2);
});

test('roam-search-03 bounded graph never claims exhaustion and early trial count is actual', async () => {
    let r=await bfs({init: () => 0,ops: () => [1],next: (m: number) => m+1,key: String},{reset() {},fire() {},chk: () => 0},{depth: 2});
    expect(r.complete).toBe(false); expect(r.stopReason).toBe('depth');
    let t=trials(100,() => 'bad',{failFast: true,log: false});
    expect(t.n).toBe(1); expect(t.pass).toBe(0);
});

test('roam-search-04 independent host clocks do not share time or restore twice', async () => {
    const make=() => ({setTimeout() {},clearTimeout() {}});
    let a=make(),b=make(),x=createClock(a),y=createClock(b),calls: string[]=[];
    (a.setTimeout as any)(() => calls.push('a'),4);
    (b.setTimeout as any)(() => calls.push('b'),8);
    try {await x.adv(4); expect(calls).toEqual(['a']); expect(y.now()).toBe(0); await y.adv(8); expect(calls).toEqual(['a','b']);}
    finally {x.off(); y.off();}
    let replacement=() => {}; a.setTimeout=replacement; x.off(); expect(a.setTimeout).toBe(replacement);
});

test('roam-search-05 corrupt seed archives are not an empty regression set', () => {
    let dir=mkdtempSync(join(tmpdir(),'roam-seeds-'));
    try {expect(seedLoad(join(dir,'missing'))).toEqual([]); writeFileSync(join(dir,'bad'),'not-json\n'); expect(() => seedLoad(join(dir,'bad'))).toThrow('Invalid seed archive');}
    finally {rmSync(dir,{recursive: true,force: true});}
});
