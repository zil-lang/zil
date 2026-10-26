/** In-process exploration for arbitrary actions. Durable cases use the core and Roam host. */
import {fmt} from './check';
import {seed, type Rng} from './data';
const log=(...args: any[]) => (globalThis as any).console?.log(...args), Jsn=JSON.stringify;
const now=() => (globalThis as any).performance?.now()??Date.now();
export type TrialRep={n: number; pass: number; fail: number; first: number; ms: number; tag: string; text: string; shrink?: any;};
/** 批跑：fn(i,rnd) falsy=通过；shrink(i,rnd,fail) 可选最小复现（常配 minimize） */
export let trials=(n: number,fn: (i: number,rnd: () => number) => any,opts: {tag?: string; failFast?: boolean; log?: boolean; shrink?: (i: number,rnd: () => number,fail: any) => any;}={}): TrialRep => {
    let tag=opts.tag||'',t0=now(),bad=0,first=-1,shr: any;
    for(let i=0;i<n;i++) {
        let r=fn(i,seed(i));
        if(r) {
            bad++;
            if(first<0) {
                first=i; if(opts.log!==false) log('  FAIL seed',i,'->',typeof r==='string'? r:JSON.stringify(r));
                if(opts.shrink) {shr=opts.shrink(i,seed(i),r); if(opts.log!==false&&shr!=null) log('  SHRINK',typeof shr==='string'? shr:JSON.stringify(shr));}
            }
            if(opts.failFast) break;
        }
    }
    let executed=first>=0&&opts.failFast? first+1:n,okN=executed-bad,ms=now()-t0;
    let text=bad===0
        ? `OK ${tag? tag+': ':''}${okN} passed, 0 failed (${okN}/${executed})`
        :`FAIL ${tag? tag+': ':''}${bad} failed, ${okN} passed (${okN}/${executed}), first seed=${first}`+(shr!=null? `, shrink=${typeof shr==='string'? shr:JSON.stringify(shr)}`:'');
    return {n: executed,pass: okN,fail: bad,first,ms,tag,text,shrink: shr};
};

// —— 状态图 BFS / 随机长序列 ——
export type Step={op: string; a?: any;};
export type Fsm<M,O=Step>={init: () => M; ops: (m: M) => O[]; next: (m: M,s: O) => M; key: (m: M) => string;};
export type Drv<M,O=Step>={reset: () => any; fire: (s: O) => any; chk: (m: M) => any; drop?: () => any;};
export type WalkRep<O=Step>={tag: string; states: number; edges: number; fail: number; bad: {path: O[]; why: any;}[]; capped: boolean; complete: boolean; depth: number; stopReason: string; ms: number; text: string;};
/** 状态图穷举：每条边都 fire+chk；仅新状态入队；capped 时 text 标注未穷尽 */
export async function bfs<M,O=Step>(spec: Fsm<M,O>,drv: Drv<M,O>,{depth=8,cap=5e3,tag='',maxBad=3}={}): Promise<WalkRep<O>> {
    let seen=new Set<string>(),q: {m: M; p: O[];}[]=[];
    let bad: {path: O[]; why: any;}[]=[],edges=0,capped=false,depthLimited=false,t0=now(),path: O[]=[];
    try {
        await drv.reset();
        let m0=spec.init(); seen.add(spec.key(m0)); q.push({m: m0,p: []});
        let w0=await drv.chk(m0); if(w0) bad.push({path: [],why: w0});
        while(q.length&&bad.length<maxBad) {
            let {m,p}=q.shift()!;
            if(p.length>=depth) {if(spec.ops(m).length) depthLimited=true; continue;}
            for(let st of spec.ops(m)) {
                let m2=spec.next(m,st); path=p.concat(st); edges++;
                await drv.reset();
                for(let s of path) await drv.fire(s);
                let why=await drv.chk(m2);
                if(why) {bad.push({path,why}); if(bad.length>=maxBad) break; continue;}
                let k=spec.key(m2); if(seen.has(k)) continue;
                if(seen.size>=cap) {capped=true; continue;}
                seen.add(k); q.push({m: m2,p: path});
            }
        }
    } catch(why) {bad.push({path,why});}
    finally {try {await drv.drop?.();} catch(why) {bad.push({path,why});}}
    let ms=now()-t0,complete=!capped&&!depthLimited&&!bad.length&&!q.length;
    let stopReason=bad.length? 'failure':capped? 'cap':depthLimited? 'depth':'exhausted';
    return {
        tag,states: seen.size,edges,fail: bad.length,bad,capped,complete,depth,stopReason,ms,
        text: bad.length? `FAIL ${tag} path=${Jsn(bad[0].path)} why=${fmt(bad[0].why)}`
            :`OK ${tag} states=${seen.size} edges=${edges}${capped? ' CAPPED':''}${depthLimited? ' DEPTH-BOUNDED':''} ${ms|0}ms`
    };
}

export type FuzzCtl<S,O=Step>={init: () => S|Promise<S>; ops: (rnd: Rng,s: S) => O|undefined; run: (s: S,st: O) => any; chk: (s: S,st?: O) => any; done?: (s: S) => any; drop?: (s: S) => any;};
export type FuzzRep<O=Step>={tag: string; n: number; fail: number; ms: number; fails: {seed: number; i: number; why: any; log: O[]; min?: O[]; cleanup?: any[];}[]; text: string;};
export type FuzzOptions={n?: number; steps?: number; tag?: string; maxFailures?: number; minimize?: boolean; seed?: number; startSeed?: number; stride?: number;};
/** 随机长序列；默认发现一个失败就返回，缩减需显式开启。 */
export async function fuzz<S,O=Step>(ctl: FuzzCtl<S,O>,{n=200,steps=20,tag='',maxFailures=1,minimize=false,seed: one,startSeed=0,stride=1}: FuzzOptions={}): Promise<FuzzRep<O>> {
    if(!Number.isSafeInteger(maxFailures)||maxFailures<1) throw new Error('maxFailures must be a positive integer');
    let play=async (log: O[]): Promise<{bad: any; phase: string; failed: boolean;}> => {
        let s: S|undefined,bad: any=0,phase='init',caught=false;
        try {
            s=await ctl.init();
            for(let st of log) {phase='run'; await ctl.run(s,st); phase='check'; if(bad=await ctl.chk(s,st)) break;}
            if(!bad) {phase='done'; bad=await ctl.done?.(s);}
        } catch(e) {bad=e; caught=true;}
        finally {try {await ctl.drop?.(s as S);} catch(e) {if(!caught&&!bad) {bad=e; phase='cleanup'; caught=true;}} }
        return {bad,phase,failed:caught||!!bad};
    };
    // Replay creates new Error instances: match explicit propertyId, otherwise exact
    // text/name/message within the same lifecycle phase. This heuristic is not a
    // semantic property identity; unknown objects are deliberately not shrunk.
    const kind=(w: any,phase: string): string|undefined => typeof w?.propertyId==='string'&&w.propertyId? 'property:'+w.propertyId:
        typeof w==='string'? phase+':'+w:w instanceof Error? phase+':'+w.name+':'+w.message:undefined;
    let t0=now(),fails: FuzzRep<O>['fails']=[],i=0,N=one!=null? 1:n;
    for(;i<N;i++) {
        let sd=one!=null? one:startSeed+i*stride,rnd=seed(sd),log: O[]=[],bad: any=0,bi=-1,s: S|undefined,cleanup: any[]=[],phase='init',caught=false;
        try {
            s=await ctl.init();
            for(let k=0;k<steps;k++) {phase='generate'; let st=ctl.ops(rnd,s); if(st===undefined) break; log.push(st); bi=k; phase='run'; await ctl.run(s,st); phase='check'; if(bad=await ctl.chk(s,st)) break;}
            if(!bad) {phase='done'; bad=await ctl.done?.(s); if(bad) bi=-1;}
        } catch(e) {bad=e; caught=true;}
        finally {try {await ctl.drop?.(s as S);} catch(e) {cleanup.push(e); if(!caught&&!bad) {bad=e; phase='cleanup'; caught=true;}} }
        if(caught||bad) {
            let min: O[]|undefined;
            if(minimize) {let cur=log.slice(),k0=kind(bad,phase); if(k0!==undefined) for(let j=cur.length-1;j>=0;j--) {let t=cur.slice(); t.splice(j,1); let b2=await play(t); if(b2.failed&&kind(b2.bad,b2.phase)===k0) cur=t;} min=cur;}
            fails.push({seed: sd,i: bi,why: bad,log,...(min? {min}:{}),...(cleanup.length? {cleanup}:{})}); if(fails.length>=maxFailures) {i++; break;}
        }
    }
    let ms=now()-t0;
    return {
        tag,n: i,fail: fails.length,ms,fails,
        text: fails.length? `FAIL ${tag} seed=${fails[0].seed} i=${fails[0].i}${fails[0].min? ` min=${Jsn(fails[0].min)}`:''} why=${fmt(fails[0].why)}`:`OK ${tag} ${i} seeds, ${ms|0}ms`
    };
}

/** Fsm+Drv → fuzz 薄封装：模型先行 next 再 fire 真实驱动，chk 对拍；清理走 drv.drop */
export let fuzzFsm=<M,O=Step>(spec: Fsm<M,O>,drv: Drv<M,O>,opts: FuzzOptions={}) =>
    fuzz<{m: M;},O>({
        init: async () => {await drv.reset(); return {m: spec.init()};},
        ops: (rnd,s) => {let o=spec.ops(s.m); return o.length? o[(rnd()*o.length)|0]:undefined;},
        run: async (s,st) => {s.m=spec.next(s.m,st); await drv.fire(st);},
        chk: s => drv.chk(s.m),
        done: s => drv.chk(s.m),
        drop: () => drv.drop?.(),
    },opts);
