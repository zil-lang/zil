// logic-one-sight, name-shorter, pf-higher  ┄┄Just for pro, but user api easy
// iXXX or $xxx or CAPS for glob var，and CAPS for symbol too
// 涉及过程：建域、开窗(收集)、运行(响应或普通)、挂载(物化到dom树、回填机制)、reclac、清理cs、清理dom
// 作用域要求：谁创建谁销毁，w.res只是物化结果，w.cs负责子项、清理项挂载
import {_,doc,lisn,Obj,oHas,oKeys,isFn,isArr,isObj,isMap,isStr,isBsc,raf,kebab,Min,Max,nop,isIdx,isAsy,tyAsy,oDefs,okr,oAsg,oDes,ONCE,G} from "./__";
import {$ops,setMutReplay,type Op} from "./cmd";// 录制槽归 cmd 自有自写（live binding 直读）；mut 重放桥经 setMutReplay 反向注入（cmd 不识代理符号，纯 cmd 应用不载内核）
import {$zss,$cls} from './zss';
export const SB=Symbol,BSC=SB.toPrimitive,ITR=SB.iterator;
// export const KPU=SB('kpu'), SIG=SB('sig'), OBS=SB('obs'), RAW=SB('raw'), PRX=SB('prx');// for dev env
// for prod env；LEN=长度信号
export const KPU=SB(),SIG=SB(),OBS=SB(),RAW=SB(),PRX=SB(),LEN=SB();
// $b-bond, $w-reg-scp for reactive graph
let $b: num|_,$w: W|_,$gc: W[]|_;
// hk：for ssr called in runSsr/hydrate sync period
export let $hk={} as {sig?: (s: any) => any,calc?: (w: any) => any,err?: (e: any,w?: any) => any;};
let thr=e => {throw e;};// just inline for minify
// 初始化使用[w]，[].push预留17槽影响性能
export let reg=(w,up: any=$w) => (up&&(w.up=up,up.cs? up.cs.push(w):up.cs=[w]),w),ent=(w,pw=$w) => ($w=w,pw),onClr=reg;
export let frg=() => doc.createDocumentFragment();
export let txt=(v?,d=new Text) => (d.data=v===!!v? '':v??'',d);

export interface Sig<T=any> extends S<T> {}
export interface OSig<T=any> extends S<T> {}
export interface ASig<T=any> extends Sig<T> {e: any;}
// 形状：生命期必写的键在构造器/字面量里预占（in-object 一次成形），冷键（op/_s/st/e/it/asy）懒加；见 .agent/pf-shape.md
export class Sig<T=any> {_v: T; wh; wt; g; op; constructor(v: T,sys?: bool) {this._v=v; sys||$hk.sig?.(this);} }
class CSig extends Sig {f; t=4; h=1; vn=0; up; dty; dt; dh; nq; cs; constructor(f,sys?: bool) {(super(_,sys) as any).f=f;} }
export class OSig<T=any> {
    tg; k; wh; wt; g; op; constructor(tg,k) {this.tg=tg; this.k=k;}
    get _v(): T {return this.tg[this.k];} set _v(v: T) {this.tg[this.k]=v;}
}
export class MSig<T=any> extends OSig<T> {
    get _v(): T {return this.tg.get(this.k);} set _v(v: T) {this.tg.set(this.k,v);}
}
// 值订阅 $/v，异步状态订阅 s，当前lww tx写入s.op 
let _sig_=Sig.prototype,$des=oDes({
    [SIG]: 1,[BSC]() {return read(this);},
    set $(v) {write(this,v);},set(v) {write(this,v);},get: read,
    get v() {this.dty&&recalc(this); $b&&bond(this); return this.op? this.op.v:this._v;},// 读旧：fst=1 吞 pending sig、真错照抛
    get s() {return this._s??=new Sig(this.st,1);},
    toJSON() {return read(this);},
});
$des.$.get=read;[Sig,OSig].forEach(s => oDefs(s.prototype,$des));
export function $(s,v?) {return arguments.length<2? isSig(s)? read(s):s:isSig(s)? (write(s,v),v):v;}
export function sig<T>(f: () => Promise<T>|AsyncGenerator<T>): ASig<T>;
export function sig<T>(f: () => T): Calc<T>;
export function sig<T>(v?: T): Sig<T>;
export function sig<T>(v: T|(() => T)) {return isFn(v)? rxFn(v,1):new Sig(v);}// 异步重载在前：Promise 返回函数同时匹配 ()=>T
export let isSig=(s: any): s is Sig => s?.[SIG]===1;

// pull-flush 模型，以及dty&&recalc防止菱形/多源读  
function read(s=this) {try {s.h>$h&&flush(s.h-1); s.dty&&recalc(s,1);} finally {$b&&bond(s);} if(s.st&APE) throw s; return s._v;}

// write：0 等值、1 已推、2 非乐观延后，tothink考虑三元压缩
function write(s: S,v: any,t: Tx|_=$tx): 0|1|2|_ {
    let op=s.op,o=s._v;
    if(t) {
        if(t.abt) return;
        if(t.log) {
            if(op?.tx===t) {if(op.v===v) return; op.v=v;}
            else {if(!op&&v===o) return; t.log.push(s.op={s,o,v,tx: t});}
            return t.opt? (s._v=v,trig(s),1):2;
        }
    }
    if(o===v&&!op) return 0; s.h||$ops?.push({s,o,v}); s.op=_; s._v=v; trig(s); return 1;
}
let write2=(s: S,v: any,t: Tx|_=$tx,op=s.op,o=s._v): 0|1|2 => t?.abt? 0:t?.log?
    (op?.tx===t? op.v===v? 0:(op.v=v,1):!op&&v===o? 0:(t.log.push(s.op={s,o,v,tx: t}),1))&&(t.opt? (s._v=v,trig(s),1):2):
    o===v&&!op? 0:(s.h||$ops?.push({s,o,v}),s.op=_,s._v=v,trig(s),1);

// abort：停流、升代、摘 AP；有 f 则置冷可重取
export let abort=(s: C) => (s.it?.return?.(),s.it=s.dty=_,s.vn++,s.st&&aSet(s,s.st&~AP),s.f&&(s.dty=1));
// calc重跑；!up吞错以免打断flush；入口清脏、推消费钟   
let recalc=(w: C,up?) => {
    w.dty=_; $g++; w.cs?.forEach(clr1,w.cs=_);
    let v,t,a,pb=$b,pw=$w; w.dt=_;
    try {
        $w=w,$b=w.t,(w.vn++,v=w.f(w),t=tyAsy(v))? (aSet(w,w.st&AV|AP),$tx&&txPnd($tx),aCalc(w,v,w.vn,$tx,t===2)):
            (a=w.st&APE,a&&aSet(w,AV),isSig(w)&&!write(w,v)&&a&&trig(w));
    }
    catch(e) {
        if(isSig(e)&&e.st&APE) {
            e.st&AP? aSet(w,w.st&AV|AP):aSet(w,w.st&AV|AE,e.e); up&&pb&&bond(w,pw);
            if(up) throw e; trig(w);// 吞错也推，下游才离 AP
        }
        else if(up) throw e; else onErr(e,w);
    }
    finally {$b=pb,$w=pw,bdTrim(w);}
};
/** 勿删：另一类环检测算法备份，思路是按照h跑图时回溯式tk计数，使用普通高低位数字但是不用bit，bit有32限制*/
// let $g=1,$max=0,allow=(n: D|W,v=n.tk||0) => ((v-v%256)/256!==$g? v=$g*256+$max:v)%256&&(n.tk=v-1);

// trig：叶子记钟去重，钟随消费（recalc/LQ 清脏）推进后才能再扫，新边当场摘标
function trig(s: S) {let b=s.wh,w; if(!b) return; if(!s.h) {if(s.g===$g) return; s.g=$g;} for(;b;b=b.nw) w=b.w,w.vn&&!w.dty&&(w.dty=1,w.t&4? hq(w):lq(w,w.l));}
export let raw=(s: any) => (isSig(s)&&(s=s._v),s?.[RAW]??s);
// sub：订值并挂 owner
export let sub=(s: S,f) => bond(s,reg({f,vn: 1,l: 1,up: _,dt: _,dh: _,dty: _} as W));
export let tog=(s: S) => s.$=!s.$;
export let toSig=<T>(tg,v?: T): S<T> => isSig(tg)? tg:sig(tg??v); // 可写值归一为 sig
// 高频优先，w.dt为生效游标、ontrack置空，对象池优化不考虑
export let bond=(d: S,w: W=$w) => {
    let pd=w.dt,vn=w.vn,nd=pd? pd.nd:w.dh;
    if(d===nd?.d) return nd.vn=vn,w.dt=nd; if(d===pd?.d) return pd.vn=vn,pd;
    let pw=d.wt; if(pw&&pw.vn===vn&&pw.w===w) return pw; if(w.h<d.h+1) w.h=d.h+1,w.wh&&hUp(w,w.h+1);// 新边校高
    let b=w.dt=d.wt={vn,w,d} as WD; d.h||(d.g=_);// 新边摘钟
    nd&&(b.nd=nd,nd.pd=b); pd? (b.pd=pd,pd.nd=b):w.dh=b; pw? (b.pw=pw,pw.nw=b):d.wh=b;
    return b;
};
// hUp：出边抬高，依赖循环超阈报警
let hUp=(w: W,h: num) => {for(let b=w.wh,x;b;b=b.nw) (x=b.w).t&4&&x.h<h&&(x.h=h,h>1e3? onErr(new Error('zit_dep_cycle'),w):x.wh&&hUp(x,h+1));};
let unbond=({w,d,pw,nw,pd,nd}: WD) => (pw? pw.nw=nw:d.wh=nw,nw? nw.pw=pw:d.wt=pw,pd? pd.nd=nd:w.dh=nd,nd? nd.pd=pd:w.dt=pd,nd);
let bdClr=(w: W,b=w.dh) => {for(;b;) b=unbond(b);};
let bdTrim=(w: W,b=w.dt) => {b=b? b.nd:w.dh; for(;b;) b=unbond(b);};

// asy state, valued,pnd,err,skipped    
export let AV=1,AP=2,AE=4,APE=6,SKP=SB();
let onErr=(e: any,w?: W) => ($hk.err??console.error)(e,w);// 全局错误出口配置
let aSet=(w: C,st,e?) => (w.st=st,w.e=e,w=w._s as any)&&w._v!==st&&(w._v=st,trig(w));// 写异步态
let aDone=(w: C,wr: 0|1|2|_,ap?) => {aSet(w,AV); ap&&!wr&&trig(w);};// AP→AV 等值补推
let aSkp=(w: C,force?) => {if(w.st&AP&&(force||!w.dty)) aSet(w,w.st&~AP),trig(w);};
// aCalc：异步叶子 settle；vn/abt 失配丢弃
let aCalc=async (w: C,ret: any,vn: num,t: Tx|_,itr: bool) => {
    let pn=t,wr; w.asy=1; itr&&(w.it=ret);
    try {
        if(itr) {
            for await(let x of ret) {
                if(w.vn!==vn||t?.abt) return ret.return?.(),w.vn===vn&&t?.abt&&aSkp(w,1);
                if(x!==SKP) wr=write(w,x,t),w.st&AP&&(aDone(w,wr,1),pn?.log&&(txPnd(pn,0),pn=_));
            }
            if(w.vn===vn&&!t?.abt) aSkp(w);
        }
        else {
            let x=await ret; return w.vn!==vn||t?.abt? w.vn===vn&&t?.abt&&aSkp(w,1)
                :x===SKP? aSkp(w):(wr=write(w,x,t),w.vn===vn&&!t?.abt&&aDone(w,wr,w.st&AP));
        }
    }
    catch(e) {if(w.vn===vn&&!t?.abt) {aSet(w,w.st&AV|AE,e); t?.log&&(t.fail=1); trig(w);} }
    finally {itr&&w.it===ret&&(w.it=_); pn?.log&&txPnd(pn,0);}
};
// asy mount
function aMnt(w: W,pr: P,pa?: Node,nx?: Node,vn=++w.vn) {
    pa&&mnt(w,pa,nx,'');
    okr(pr).then(r => w.vn===vn&&mnt(w,pa,nx,r),e => w.vn===vn&&onErr(e,w));
}
// retry：升代重取；冷或未脏入队
export let retry=(...ss: C[]) => {for(let s of ss) if(s?.f) {let cold=!s.vn,err=s.st&AE; s.it?.return?.(),s.it=_; s.vn++,aSet(s,s.st&AV|AP); (cold||!s.dty)&&(s.dty=1,hq(s)); err&&trig(s);} };
// fb：内容槽。只接自身 f 栈上的异步信号，画面走 mnt。uf 留给服务端自管
export let fb=(f,ph,eb?) => reg({
    __: 'fb',t: 1,vn: 0,ph,eb,uf: f,p: sig(0),h: 1,
    f(w) {
        let pw=$w,pb=$b; $w=w; $b=w.t;
        try {let r=f(w); w.p.set(0); return r;}
        catch(e) {
            if(!(isSig(e)&&e.st&APE)) throw e;
            bond(e,w); w.p.set(e.st&AP? 1:0);
            if(e.st&AP) return w.res? w.res:isFn(ph)? ph(e):ph;
            let er=eb??ph; return isFn(er)? er(e.e,e,() => fbSet(w)):er;
        }
        finally {$w=pw; $b=pb;}
    },
}) as Fb;
let fbSet=(b: any) => (b.vn=1,b.dty=1,lq(b));
// 勿删，找到最近边界逻辑 ┄┄ 则记 e、杀毒、重跑；否则 0，此逻辑为老逻辑备份
let fbUpr=(w: any=$w) => {for(;w&&w.__!=='fb';w=w.up); return w;};
let fbErr=(e: any,w: any,b=fbUpr(w)) => b&&b!==w? (b.e=e,gc(w),fbSet(b),1):0;

let txPnd=(t: Tx,up=1) => t.log&&(up? ++t.pn:--t.pn||txEnd(t,1),t.p?.set(t.pn));// 未决计数；到 0 提交
export let pnd=(t: Tx) => t.p??=sig(t.pn);
let txStep=(c: Op,tg,s=c.s) => s.op===c&&(s.op=_,s._v!==tg&&(s._v=tg,trig(s)),1);
export let onTx: ((t: Tx,ok: bool) => void)|_;
// txEnd：提交或回滚并 settle；rf 走 reject
let txEnd=(t: Tx,ok?: any,e?: any,rj?: bool) => {
    if(!t.log||ok&&(t.pn||t.opt===2)) return t;
    if(ok&&(t.fail||t.abt)) ok=0;
    let log=t.log,ops=ok&&$ops; t.log=_; t.pn=0; t.p?.set(0);
    if(ok) for(let c of log) {txStep(c,c.v)||(t.skp??=[]).push(c); ops?.push(c);}
    else {t.abt=1; for(let i=log.length;i--;) txStep(log[i],log[i].o);}
    $tx=_; onTx?.(t,ok); rj? t._rej?.(e):t._res?.(t);
    return t;
};
let txNest=(t: Tx,f: Fn) => isAsy(f=f(t))? (txPnd(t),okr(f).finally(() => txPnd(t,0))):t;// 复用外层；async 计入 pn
let txRun=(t: Tx,f: Fn,eb: Fn) => {$tx=t; try {f=f(t); flush(); return f;} catch(e) {eb(t); throw e;} finally {$tx=_;} };
// cancel：只回滚
export let cancel=(t: Tx) => t?.log&&txEnd(t,0);
let ABORT=new DOMException('ABORT');
// tx：开会话；无参返当前
export function tx<T>(f?: (t?: Tx) => T,abs?: AbortSignal): Tx|_ {
    if(!f) return $tx; if($tx) return txNest($tx,f);
    let t={log: [],pn: 0} as Tx;
    if(abs?.aborted) t.abt=1;
    let ret=txRun(t,f,txEnd);
    let abort=() => txEnd(t,0,abs?.reason??ABORT,1);
    abs?.addEventListener('abort',abort,ONCE);
    if(isAsy(ret)) {
        let p=new Promise((o,n) => {t._res=o; t._rej=n;}) as any; txPnd(t);
        okr(ret).then(_ => (txPnd(t,0),txEnd(t,1)),e => txEnd(t,0,e,1));
        p.cancel=abort; p.tx=t;
        return p;
    }
    return t.pn? t:txEnd(t,1);
}
// action：跨 yield 保持同一写会话
export function action<Ag extends any[],R>(gen: (...ag: Ag) => Generator<unknown,R,unknown>) {
    return (...ag: Ag) => {
        let outer=$tx,nest=!!outer,t=outer||{opt: 1,log: [],pn: 0} as Tx,it=gen(...ag),res!: any,rej!: any;
        let pr=new Promise<R>((o,n) => {res=o; rej=n;}) as Promise<R>&{cancel?(): void; tx?: Tx;};
        pr.tx=t;
        let done=(v: any,e?: any,f?: bool) => {// 顶层随会话 settle；嵌套即解析
            if(nest) return f? rej(e):res(v);
            t._res=() => res(v); t._rej=rej;
            f? txEnd(t,0,e,1):txEnd(t,1);
        };
        let step=(v?: any,err?: bool) => {
            let pre=$tx; $tx=t;
            try {
                while(1) {
                    let r=err? it.throw(v):it.next(v); err=0;
                    if(isAsy(r)) throw new Error('action use yield not await)');
                    if(r.done) {flush(); return done(r.value);}
                    let x: any=r.value;
                    if(x?.then) {flush(); txPnd(t); (x as Promise<any>).then(yldStep,yldErr); return;}
                    v=x;
                }
            } catch(e) {done(_,e,1);}
            finally {$tx=pre;}
        };
        let errStep=(e: any) => step(e,1);
        let yldStep=(r: any) => {t.log&&(--t.pn,t.p?.set(t.pn)); step(r);};// 先减 pn 再步进
        let yldErr=(e: any) => {t.log&&(--t.pn,t.p?.set(t.pn)); errStep(e);};
        pr.cancel=() => t.log&&(it.return?.(_),nest? t.abt=1:cancel(t),rej(new Error('cancelled')));
        step();
        return pr;
    };
}
// tentative：opt=2 未确认；ok 降为 1 后未决归零才提交；始终返 t
export function tent(f?: (t?: Tent) => any,abs?: AbortSignal): Tent {
    if(!f) return $tx as Tent; if($tx) return txNest($tx,f) as Tent;
    let ret,t={opt: 2,log: [],pn: 0} as Tent,p=new Promise<Tent>((o,n) => {
        t._res=v => {t.then=_; o(v);}; t._rej=e => {t.then=_; n(e);};
    });
    t.ok=() => (t.opt=1,txEnd(t,1)); t.no=() => cancel(t); t.then=p.then.bind(p);
    if(abs?.aborted) t.abt=1; abs?.addEventListener('abort',t.no,ONCE);
    if(isAsy(ret=txRun(t,f,t.no))) txPnd(t),okr(ret).then(_ => txPnd(t,0),e => txEnd(t,0,e,1));
    return t;
}
export let isPnd=(...ss) => ss.some(s => ($b&&bond(s.s),s.st&AP));
export let isErr=(...ss) => ss.some(s => ($b&&bond(s.s),s.st&AE));
export let isOld=(...ss) => ss.some(s => ($b&&bond(s.s),s.st&AV));

export let latest=(s: any) => s?.v;
let kick=(s: S) => {try {s.$;} catch(e) {if(!(isSig(e)&&e.st&APE)) throw e;} };// 只吞 APE
let til=<T=any>(s: S<T>): Promise<T> => new Promise((ok,no) => {
    kick(s); let vn=s.vn,chk=() => {let st=s.st||0; if(st&AP) return 0; if(s.vn!==vn&&s.dty) return no(new Error('zit: aborted')),1; if(s.dty) return 0; if(st&AE) return no(s.e),1; return ok(s._v),1;};
    chk()||sub(s.s,w => chk()&&clr(w));
});
// until：单源返 T，多源并行返数组  
export function until<T>(s: S<T>): Promise<T>;
export function until<T>(...ss: S<T>[]): Promise<T[]>;
export function until(...ss: any[]): any {return ss.length===1? til(ss[0]):Promise.all(ss.map(til));}
// optim：乐观写，失败回滚
export let optim=<T=any>(s: S<T>,v: T,p: Promise<any>): Promise<any> => {
    let o=s._v; s.$=v; return okr(p).catch(e => {s._v===v&&(s.$=o); throw e;});
};
// withTimeout：超时 cancel 并抢先 reject
export let withTimeout=<T>(p: Promise<T>,ms: num,msg='timeout'): Promise<T> => {
    let tm,race=new Promise<T>((_o,no) => tm=setTimeout(() => (no(new Error(msg)),(p as any).cancel?.()),ms));// 先拒再 cancel，保证 timeout 错
    return Promise.race([p,race]).finally(() => clearTimeout(tm));
};

// lane：0 最高；LANES 须与 LQ 同步，$h=最低脏桶，空队=∞
let HQ: W[]=[],HT: W[]=[],$h=1/0,LANES=4,LQ: W[][]=[[],[],[],[]],$l=LANES,$sch=0,$g=0,$tx: Tx|_,$sp: MessagePort|_;
export let $q=() => [$h,HQ,HT,LQ,$sch];// 测试态访问器
let go=() => {try {flush();} finally {$sch=0;} },$pr=okr(),mi=() => ($pr.then(go),1),sch=mi;// then 作业是纯 V8 微任务，queueMicrotask 走 Blink 绑定（DevTools 插桩下差 6 倍）
let ma=(c?) => ($sp||=(c=new MessageChannel(),c.port1.onmessage=go,c.port2),$sp.postMessage(0),1);
export let schMac=(on=1) => sch=on? ma:mi;// sch挂宏任务
// hq：同桶节点互不依赖（dep.h<consumer.h 严格 ⇒ 同 h 无边），桶内任意序执行即并行安全
export let hq=(w: W,h=w.h) => w.nq? 0:(HT[h]? HT[h].nq=w:HQ[h]=w,w.nq=w,$h>h? $h=h:1,HT[h]=w,$sch||=sch());
export let lq=(w: W,l=1) => (LQ[l].push(w),$l>l? $l=l:1,$sch||=sch());
// flush：排空 HQ（可截高）；无会话再跑 LQ。钟随消费推进，叶子不进零桶
export let flush=(mx=1/0) => {
    for(let h=$h,w,n;h<HQ.length&&h<=mx;h=$h===h? ++$h:$h) {// 换桶：等号进位，否则跟 $h
        for(w=HQ[h];w;w=HQ[h]) {// 先出队再算
            n=w.nq; n===w&&(HT[h]=n=_); HQ[h]=n; w.nq=_;
            w.vn&&w.dty&&(w.h!==h? hq(w):recalc(w)); if(h>$h) break;
        }
    }
    if(mx<1/0) return;
    HQ.length=HT.length=0,$h=1/0;
    if($tx) return;

    for(let l=$l,w,q;l<LANES;) {
        q=LQ[l]; if(!q.length) {l=++$l; continue;};
        for(let i=0;i<q.length;i++) {
            if(w=q[i],w.vn&&w.dty) {
                w.dty=0; $g++; try {mnt(w);} catch(e) {onErr(e,w);}
            }
            if(l>$l) {LQ[l]=q.slice(i+1); break;}
        }
        l=l>$l? $l:(q.length=0,++$l);
    }
    $h<1/0&&flush();
};
export let idle=(f,ms=1e4) => G.requestIdleCallback? requestIdleCallback(f,{timeout: ms}):setTimeout(f,1) as any;// 无 rIC 环境（node/旧浏览器/Safari）降级
export let tick=(f=nop) => raf(_ => setTimeout(f));
// 节点定位；[] 需回填、{} 不回填
let fst=(x: any): any => !x? _:x.nodeType? x:isArr(x)? fst(x[0]):x.anc||fst(x.res),
    lst=(x: any): any => !x? _:x.nodeType? x:isArr(x)? lst(x[x.length-1]):x.enc||lst(x.res),
    mv=(pa: Node,tg,nx?: Node,a=fst(tg),e=a&&lst(tg)) => {
        if(!a||a===nx||e.parentNode===pa&&e.nextSibling===nx) return a;
        for(let n=a,c;n!==e;n=c) c=n.nextSibling,pa.insertBefore(n,nx);
        return pa.insertBefore(e,nx),a;
    };
// fn变w{}、{} 回填res、[] 回填压紧、asy变fn
export function add(pa: Node,tg,nx?: Node) {
    if(!pa||tg==_) return tg; let t,i,j;// pa检查防游离组件
    if(tg.nodeType) pa.insertBefore(tg,nx);
    else if(isBsc(tg)) pa.insertBefore(tg=txt(tg),nx);
    else if(isFn(tg)) return domx(tg,pa,nx);
    else if(isSig(tg)) return domx(FSG,pa,nx,tg);
    else if(tg.__) fst(tg)?.parentNode? mv(pa,tg,nx):tg.mnt? tg.mnt(pa,nx):mnt(tg,pa,nx);
    else if(isArr(tg)) {for(i=j=0;i<tg.length;i++) (t=add(pa,tg[i],nx))&&(tg[j++]=t); tg.length=j,j? 0:tg[0]=add(pa,'',nx);}
    else if(isAsy(tg)) return domx(() => tg,pa,nx);// 一次性 Promise：同内容 watcher 占位→settle 挂载，属主清理即弃
    return tg;
};
// atn：键→属性名（kmap 字串项即别名，函数项/无项原名）
let atn=(k,m=kmap[k]) => isStr(m)? m:k;
export let att=(el: Element,o) => {for(let k in o) k==='cls'&&$zss?.ins(o[k]),el.setAttribute(atn(k),o[k]);};
export let aft=(nd: Node,tg: Node|W) => add(nd.parentNode,tg,nd.nextSibling);
export let bef=(nd: Node,tg: Node|W) => add(nd.parentNode,tg,nd);
export let ins=(nd: Node,tg: Node|W,pre=1) => add(nd.parentNode,tg,pre? nd:nd.nextSibling);
export let emp=(tg) => (swp(tg),tg.innerHTML='');
// rem：只卸 DOM。Host 摘锚段；W 沿 res 剥到节点
export let rem=tg => {
    if(tg.nodeType) return tg.remove(),swp(tg);
    if(isArr(tg)) return tg.forEach(rem);
    if(tg.enc) {for(let c=tg.anc,n,e=tg.enc.nextSibling;c&&c!==e;c=n) n=c.nextSibling,rem(c); return;}
    rem(tg.res);
};
let _swp=(d: Node,w: any=d.rt,c=d.firstChild) => {for(;w;w=w.rt) clr(w,1); for(;c;c=c.nextSibling) _swp(c);};// 节点即链头：dom.rt→w.rt→… 同名单链
export let swp=nop as typeof _swp;
let DMR: bool;

// gc：标死入队延清
let _gc=() => {for(let i=0;i<$gc.length;i++) clr($gc[i] as any); $gc=_;};
let _gc_bak=(n=1e5,i=0,w?: W) => {while(w=$gc.pop()) if(clr(w),i++>n) return queueMicrotask(_gc); $gc=_;};
export let gc=(w: C) => (w.it?.return?.(),w.it=w.f=_,w.vn=0,($gc??=($pr.then(_gc),[])).push(w));

// clr：只 W。位1 卸画面（有 clr 则自管），再拆 cs。d=祖先已摘。表项可以是函数
export let clr=(w: any,d?) => {
    if(isFn(w)) return w();
    w.it?.return?.(),bdClr(w),w.vn=0; let r=w.res,c=w.cs;
    let k=w.t^2&&r&&(!d||isArr(r)); k&&rem(r),w.it=w.f=w.cs=_,k&&(w.res=_),c?.forEach(clr1);
};
let clr1=(x: any) => clr(x,1);

export let runx=(w,pw=$w,pb=$b) => {try {return $w=w,$b=w.t,++w.vn,w.f(w);} finally {$b=pb; $w=pw;} };// track
export let nox=(f,w?,pb=$b) => {try {return $b=_,f(w);} finally {$b=pb;} };// untrack
export let scpx=(f,w={f},pw=$w) => {try {return f($w=w as any),w;} finally {$w=pw;} };
export let spy=(...ag) => $b&&ag.forEach(s => bond(s));
// rxFn：lazy 标脏待读；sys 不占创建序
export function rxFn(f: Fn,lazy?: bool,sys?: bool): C {
    let w=reg(new CSig(f,sys)) as C;
    if(sys||!$hk.calc?.(w)) lazy? w.dty=1:runx(w);
    return w;
};

// 已经包含在mnt内部
let put=(w: W,r,pa?: Node,nx?: Node,o: any=w.res) => {
    r=isBsc(r)? txt(r,o?.nodeType===3? o:_):r; if(r===o) return;
    if(o) pa??=lst(o).parentNode,nx??=lst(o).nextSibling,rem(o);
    w.res=add(pa,r,nx);
};
// 开窗、运行、挂载,  分清场景 ①首插add有pa、②复跑无pa ③游离态 ④直接赋值res
function mnt(w: W,pa?: Node,nx?: Node,r?) {
    let o: any=w.res,vn=++w.vn,pw=$w,pb=$b; try {
        $w=w,$b=w.t;
        if(arguments.length<4) vn>1? (w.dt=_,w.cs?.forEach(clr1,w.cs=_),r=w.f(w),w.t&&bdTrim(w)):r=w.f(w);
        if(w.t&2) return w;
        if(r===clr) return r(w);
        if(isBsc(r)) r=txt(r,o?.nodeType===3? o:_);
        else if(isAsy(r)) okr(r).then(n => w.vn^vn||mnt(w,pa,nx,n),e => w.vn^vn||onErr(e,w)),r='';
        if(o) {if(r===o) return; nx=lst(o),pa=nx.parentNode,nx=nx.nextSibling,rem(o);}
        w.res=add(pa,r,nx); return w;
    } finally {$b=pb,$w=pw;}
}
// 建域、开窗、运行、挂载，track/runx无法递归处理res得自行调用
export function domx(f,pa: Node,nx?: Node,sr?: S) {
    let w=reg({f,t: 1,vn: 1,sr,up: _,res: _,dt: _,dh: _,dty: _,cs: _} as W);
    return mnt(w,pa,nx);
};
export function attx(f,res: Node,kr?,vr?,ns?,sr?: S) {
    let w={f,t: 2,vn: 1,res,kr,vr,ns,sr,up: _,dt: _,dh: _,dty: _} as W;
    DMR? (w.up=$w,w.rt=res.rt,res.rt=w):reg(w);
    return runx(w),w;
};

let svg='http://www.w3.org/2000/svg',math='http://www.w3.org/1998/Math/MathML',emap={};
export let dir=(k,v) => kmap[k]=v;
// sty：单条样式落地——k 为属性名走 setProperty，无 k 整串 cssText；sig/fn 建 attx 共享 FST（kr/vr/sr 字段承载，零闭包）
let sty=(dom,k,x) => isSig(x)? attx(FST,dom,k,_,_,x):isFn(x)? attx(FST,dom,k,x):k? dom.style.setProperty(k,x):dom.style.cssText=x;
export let kmap={
    style(dom: HTMLElement,v: any) {if(v?.constructor===Obj) for(let k in v) sty(dom,kebab(k),v[k]); else sty(dom,_,v);},
    vm(dom: HTMLInputElement,vm) {
        let self=0,nm=dom.localName,t=dom.type,chk=t==='checkbox'||t==='radio';//self防循环
        let p=nm==='input'&&chk? 'checked':nm==='input'||nm==='textarea'||nm==='select'? 'value':'innerText';
        rxFn(() => ($(vm),self? self=0:dom[p]=$(vm),dom),0,1);// 先订再回写；sys 不占创建序
        dom[nm==='select'||chk? 'onchange':'oninput']=() => {self=1; $(vm,dom[p]);};
    },
    hov(dom: HTMLElement,v) {
        let s=isSig(v),w=s||reg({t: 1,f: v,vn: 0,res: _,cs: _,up: _} as W);
        lisn(dom,'mouseenter',() => s? v.$=1:mnt(w,dom));
        lisn(dom,'mouseleave',() => s? v.$=_:clr(w));
    },
    cls: 'class',clx: 'class',sty: 'style',w: 'width',h: 'height',v: 'value',dis: 'disabled',
    html: 'innerHTML',tsf: 'transform',tsn: 'transition',ph: 'placeholder',vb: 'viewBox',
    ptr: 'onpointerdown',clk: 'onclick',chg: 'onchange',inp: 'oninput',
    mdn: 'onmousedown',mup: 'onmouseup',dn: 'onkeydown',up: 'onkeyup',edit: 'contentEditable',
};
let domKV=(dom,k,v,ns) => {
    if(k==='class') {$zss&&!$cls.has(v)&&$zss.ins(v); ns? dom.setAttribute(k,v):dom.className=v;}
    else k in dom&&(!ns||!isObj(dom[k]))? dom[k]=v:dom.setAttribute(k,v);
    return dom;
};
let FSG=w => w.sr.$,FSA=w => domKV(w.res,w.kr,w.sr.$,w.ns),FKV=w => domKV(w.res,w.kr,w.vr(),w.ns),
    FST=(w,s=w.res.style,v=w.sr? w.sr.$:w.vr()) => w.kr? s.setProperty(w.kr,v):s.cssText=v;// 样式四态（cssText/属性 × sig/fn）共用一函
export function rxProps(dom: El,props: any,ns?) {
    for(let k in props) {
        let v=props[k]; if(v==null||k[0]==='_') {dom[k]=v; continue;}
        let fn,km,k2,ret,isf=isFn(v);
        if(km=kmap[k]) {// 一次查表：字串项即别名落属性名（cls→class），别名再指指令（sty→style）二跳
            isStr(km)&&(k=km,km=kmap[k]);
            if(isFn(km)) {(ret=km(dom,v))&&(ret.rt=dom.rt,dom.rt=ret); continue;}// 指令清理挂节点 rt 链，随 swp 清
        }
        if(!isf&&!isSig(v)) {domKV(dom,k,v,ns); continue;}
        if(k2=k.slice(0,2),k2==='on') {
            let ev=emap[k]||=k.toLowerCase();
            if(isf) {!(ev in dom)||dom[ev]? dom.addEventListener(ev.slice(2),v as any):dom[ev]=v; continue;}
        }
        else if(k2==='da'&&k.slice(0,5)==='data-'&&isf) fn=() => dom.setAttribute(k,v());
        else if(k2==='k_') {/*预留处理*/;}
        isSig(v)? attx(FSA,dom,k,v,ns,v):attx(fn??FKV,dom,k,v,ns);
    }
}

export function nsTags(ns?: string): Tags {
    return new Proxy({},{
        get: (iTag,tag: string) => iTag[tag]??=(...ag) => {
            let dom=ns? doc.createElementNS(ns,tag):doc.createElement(tag);
            if(ag.length) {
                let i=ag[0]?.constructor===Obj&&!ag[0].__? 1:0;
                if(i) rxProps(dom as any,ag[0],ns);
                for(;i<ag.length;i++) add(dom,ag[i]);
            };
            return dom;
        }
    }) as Tags;
}
export let useZit=({$}={}) => ($&&oDefs(_sig_,{[$]: $des.$}),tags);
export let tags=nsTags() as Tags;
export let svgs=nsTags(svg) as Tags;
export let maths=nsTags(math) as Tags;
// for ssr
export let ssrTags=(t: any,s?: any,m?: any) => (s&&(svgs=s),m&&(maths=m),tags=t);

//***********rx plus**************************
export function rx<T extends object>(tg: T,f?: (v: any,k: any) => any): Rx<T> {// f：键值回调
    if(isFn(tg)) return rxFn(tg as (...ag: any[]) => any) as any;
    if(f) {
        tg=tg[RAW]||tg;
        hst(tg,_,(h,c,op) => {if(c.a) for(let i=0,s=c.st||0;i<c.a;i++) f(h.tg[s+i],s+i); else op==='set'&&f(c.ag[1],c.ag[0]);});
        (tg as any).forEach(f);
    }
    return mkPrx(tg);
};
// 容器级/长度级/键级懒信号
let sBond=(tg: any) => $b&&bond(tg[SIG]??=sig(0)),nBond=(tg: any,v) => ($b&&bond(tg[LEN]??=sig(v)),v),kSig=(tg: any,k) => (tg[KPU]??={})[k]??=new OSig(tg,k);
// mkPrx：raw 上唯一代理
export let $trap: any={
    get(tg,k,prx) {
        let v,f;
        if(typeof k==='symbol') {
            if(k===RAW) return tg;
            if(k===BSC) return sBond(tg),() => tg+'';
            if(k===ITR) return sBond(tg),tg[ITR]?.bind(tg);
            if(k===PRX) return prx;
            return tg[k];
        }
        if(k==='toJSON') return () => (sBond(tg),tg);// 序列化走 raw 快照，与 BSC/ITR 同订容器级
        if(isArr(tg)) {
            if(k==='length') return nBond(tg,tg.length);
            if(f=amu[k]) return f;
            if(!isIdx(k)) return tg[k];
        }
        else if(isMap(tg)) {
            if(f=mmu[k]) return f;
            if(k==='size') return nBond(tg,tg.size);
            return tg[k]?.bind?.(tg)??tg[k];
        }
        v=tg[k]; $b&&bond(kSig(tg,k));
        return asPrx(v);
    },
    set(tg,k,v,prx) {
        let arr=isArr(tg);
        if(arr&&k==='length') {
            let old=tg.length; v|=0;
            if(v<old) {// 截断走 splice/clear；先落长度再通知（勿 slice 尾段：nrm/host 只用 st/d）
                if(_amu) {v? prx.splice(v,old-v):prx.clear(); return 1;}
                tg.length=v; mut(tg,v? 'splice':'clear',(tg[OBS]||tg[KPU])&&{ag: [v,old-v]},_,old,nrm); return 1;
            }
            tg.length=v; tg[LEN]?.set(v); arrTrig(tg); return 1;
        }
        let idx=arr&&isIdx(k),s=kSig(tg,k);
        if(arr||oHas(tg,k)) write(s,v);
        else {tg[k]=v; $ops?.push({s,v}); s.op=_; trig(s); arrTrig(tg);}// 新键先落值（write 等值会丢）
        if(idx) {arrTrig(tg); tg[LEN]?.set(tg.length);}
        return 1;
    },
    deleteProperty(tg,k) {
        let kpu=tg[KPU],ret=delete tg[k]; if(!ret||oHas(tg,k)) return ret;
        kpu?.[k]&&trig(kpu[k]);
        (!isArr(tg)||isIdx(k))&&arrTrig(tg);
        return ret;
    },
    ownKeys(tg) {sBond(tg); return Reflect.ownKeys(tg);},
},mkPrx=tg => tg[PRX]??=new Proxy(tg,$trap);
let asPrx=(v: any) => isBsc(v)? v:mkPrx(v);
let arrTrig=(tg: any) => tg[SIG]&&tg[SIG].$++;
// nrm：规范化 mut ctx
let nrm=(k: string,c: HCtx,pre: num,_r: any,tg: any,ag=c.ag): HCtx => {
    let a=k==='push'||k==='unshift'? ag.length:k==='splice'? Max(0,ag.length-2):0,
        st=k==='push'? pre:k==='pop'? tg.length:k==='splice'? ag[0]<0? Max(0,pre+ag[0]):Min(ag[0]||0,pre):0;
    return c.st=st,c.d=pre-tg.length+a,c.a=a,c;// 增段读 h.tg[st..)，不拷 add
};
// mut：结构变更广播
let mut=(tg,k: string,c: HCtx|_,ret: any,pre: num,nm?: Fn,hf?: Fn,op?: any,h=tg[OBS],kpu=tg[KPU]) => {
    if(c) {
        nm?.(k,c,pre,ret,tg);
        for(;h;h=h.rt) h.prx? h.anc?.parentNode&&hf?.(h,c):h.fx?.(h,c,k);
        if(hf&&kpu) for(k in kpu) isIdx(k)&&+k>=c.st&&trig(kpu[k]);
        op&&(op.ret=ret,op.c=c);
    }
    tg[LEN]?.set(tg.length??tg.size);
    arrTrig(tg);
};
// hst：建host，obs-sym考虑普通{}也可容器，若仅[]/map使用.rt更简单
let hst=(tg,prx,fx?: H['fx'],__?: H['__']): H => tg[OBS]={__,tg,prx,fx,clr: hClr,rt: tg[OBS]} as H;
// hSwp：清段 DOM 与行作用域。段独占父节点（锚外无兄弟）时 replaceChildren 整批摘除（比逐节点再快 12%，clear 1k 落后 vanilla 的全部差距在此），DMR 先沿兄弟只清不摘；否则逐节点
export let hSwp=(h: H,i=0,j=h.res?.length||0,ws=h.res,a=h.anc,e=h.enc,one=a&&!a.previousSibling&&!e.nextSibling) => {
    if(a) {if(one) {if(DMR) for(let c=a.nextSibling,n;c&&c!==e;c=n) n=c.nextSibling,swp(c); a.parentNode.replaceChildren(a,e);} else for(let c=a.nextSibling,n;c&&c!==e;c=n) n=c.nextSibling,rem(c);}
    for(;i<j;i++) ws[i]&&=clr(ws[i],1);
};
// hClr：host 自退物理段、观察链与行作用域；d 表上层已摘9
export let hClr=function(this: H,d?,h=this) {
    let tg=(d||h.enc&&rem(h),h.tg),p=tg[OBS]; if(p===h) tg[OBS]=h.rt; else for(;p&&p.rt!==h;p=p.rt); p&&(p.rt=h.rt);
    h.res?.forEach(clr1); h.res=h.ix=h.anc=h.enc=_;
};
let hMnt=function(this: H,pa: Node,nx?: Node) {
    pa.insertBefore(this.anc||=txt(),nx);
    pa.insertBefore(this.enc||=txt(),nx);
    let rs=this.res,n: any=this.enc,i=rs?.length||0,w;
    if(i) for(;i--;) (w=rs[i])?.res&&(mv(pa,w.res,n),n=fst(w.res)||n);// 已有行：摘过再挂只把原节点插回锚间
    else gpr(this,this.enc);
};

// ixr：同步行下标
export let ixr=({ix}: H,i=0,n=ix?.length) => {if(ix) for(;i<n;i++) ix[i]?.set(i);};
// gpr：行结果自身给出边界，只有 host 保留区间锚
export function gpr(h: H,nx: Node,st=0,n=h.tg.length-st,pa=nx.parentNode) {
    let {tg,ix,each}=h,res=h.res||=[],pw=$w,pb=$b; $b=_;
    try {
        for(let w,i=st,e=st+n;i<e;i++) {
            w=res[i]={t: 1,vn: 0,up: h,res: _,cs: _} as any;// 行=内容属主（t=1 同槽）；逻辑上级恒为 host（h.up 挂载时写），不随变更发起者漂移
            mnt(w,pa,nx,each(asPrx(tg[i]),ix? ix[i]=sig(i):_,w));
        }
    } finally {$b=pb; $w=pw;}
}
// opr：描述子编译成最终方法；hf 已按操作选定，宿主链只遍历一次
let opr=(out: any,mu: any,pfx: string,nm?: Fn,fx?: any,pf?: Fn) => {
    for(let k in mu) {
        let rf=mu[k],hf=fx?.[k],t=pfx+k;
        out[k]=function(...ag) {
            let tg=this[RAW],pre=tg.length??tg.size,op: any,c=($ops||tg[OBS]||hf&&tg[KPU])&&{ag} as HCtx;
            $ops?.push(op={t,k,v: tg,ag})&&rf&&pf?.(op,tg,ag);
            let ret=rf? rf(tg,ag,c):tg[k](...ag);
            (c||tg[LEN]||tg[SIG])&&mut(tg,k,c,ret,pre,nm,hf,op);
            op&&c&&(c.f=c.nm=_);
            return ret===tg? this:ret;
        };
    }
};
// 方法表直存最终函数；afx 是数组操作对应的行处理器
type Mu=Record<string,Fn>;
export let amu: Mu={},mmu: Mu={},afx: Record<string,Fn>={},_amu=0,_mmu=0,_rst=0;
// reorder：按 ws 对齐 DOM
let reorder=(h: H,dead?: W[],i0=0,i1=h.res?.length||0) => {
    let p=h.enc?.parentNode,nx: any=h.enc,ws=h.res,i=i1,w;
    if(p&&ws) for(;i-->i0;) nx=(w=ws[i])?.res? mv(p,w.res,nx):dead? (gpr(h,nx,i,1),fst(ws[i].res)):nx;
    if(dead) for(w of dead) w&&clr(w);
};
let mutReplay=(op: any,k: string,ag: any[]) => mkPrx(op.v)[k](...ag);
// useArr：数组变更与行宿主
export let useArr=() => {
    if(_amu++) return;
    setMutReplay(mutReplay);
    let rev=(a) => a.reverse(),grow=(a,st,n) => st===a.length? a.length+=n:a.splice(st,0,...Array(n)),
        sync=(h: H,c: HCtx,f=c.f) => f&&(h.n1===_? (f(h.res||=[]),h.ix&&(f(h.ix),ixr(h,0)),reorder(h)):h.rbd?.());
    let spl=(h: H,c: HCtx) => {
        let {st=0,d=0,a=0}=c,ws=h.res||=[],ix=h.ix;
        if(d) {for(let w of ws.splice(st,d)) clr(w); ix?.splice(st,d);}
        if(a) {
            let anc=ws[st]? fst(ws[st].res):h.enc; grow(ws,st,a); ix&&grow(ix,st,a);
            let cr=h.cut? h.cut(h,c):[st,a]; cr&&gpr(h,anc,cr[0],cr[1]);
        }
        h.adj? h.adj(h,c):ixr(h,st+a);
    };
    let clear=(h: H,c: HCtx) => {hSwp(h); h.res=[]; h.ix&&=[]; h.adj?.(h,c);};
    let swap=(h: H,c: HCtx,ws=h.res||=[],a=c.x,b=c.y) => {
        let f=c.f; if(!f) return; f(ws); h.ix&&(f(h.ix),ixr(h,0));
        if(c.n!==1||c.z!==1) return h.n1===_? reorder(h):h.rbd?.();
        let wA=ws[a],wB=ws[b],p=fst(wA?.res)?.parentNode;
        if(!wA||!wB) return wA!==wB&&h.rbd?.();
        if(p) {let ra=lst(wA.res).nextSibling,rb=lst(wB.res).nextSibling; mv(p,wA.res,rb); mv(p,wB.res,ra);}
    };
    oAsg(afx,{
        push: spl,pop: spl,shift: spl,unshift: spl,splice: spl,
        reverse: sync,sort: sync,move: sync,swap,clear
    });
    opr(amu,{
        push: 0,pop: 0,shift: 0,unshift: 0,splice: 0,
        reverse: (tg,_ag,c) => (c&&(c.f=rev),rev(tg)),
        sort(tg,ag,c,f=ag[0]) {
            let n=tg.length,i; if(n<2) return tg; if(!tg[OBS]) return tg.sort(f? f:(a,b) => a<b? -1:a>b? 1:0);
            let p=Array(n),t=Array(n),run=(a,i=n) => {for(;i--;) t[i]=a[p[i]]; for(i=n;i--;) a[i]=t[i]; return a;}; for(i=n;i--;) p[i]=i;
            p.sort(f? (a,b) => f(tg[a],tg[b]):(a,b) => (a=tg[a])<(b=tg[b])? -1:a>b? 1:0);
            c.f=run; return run(tg);
        },
        move(tg,ag,c,x=ag[0]|0,y=ag[1]|0,n=ag[2]===_? 1:ag[2],len=tg.length) {
            if(n<=0||x===y) return tg;
            x<0&&(x+=len); y<0&&(y+=len); if(x<0||x>=len) return tg;
            n=Min(n,len-x); y=Min(y<0? 0:y,len); y>x&&(y-=n);
            let f=a => (a.splice(y,0,...a.splice(x,n)),a); c&&(c.f=f); return f(tg);
        },
        swap(tg,ag,c,x=ag[0],y=ag[1],xn=ag[2]===_? 1:ag[2],yn=ag[3]===_? 1:ag[3],len=tg.length) {
            if(x===y||xn<=0||yn<=0) return tg;
            x<0&&(x+=len); y<0&&(y+=len); if(x<0||y<0||x>=len||y>=len) return tg;
            if(x>y) [x,y,xn,yn]=[y,x,yn,xn];
            let f=a => {
                if(xn===1&&yn===1) [a[x],a[y]]=[a[y],a[x]];
                else {let d=y+yn-xn,t=a.splice(y,yn),t2=a.splice(x,xn,...t); a.splice(d,0,...t2);}
                return a;
            };
            c&&(c.f=f,c.x=x,c.y=y,c.n=xn,c.z=yn); return f(tg);
        },
        clear: (tg,_ag,_c,n=tg.length) => (tg.length=0,n),
    },'arr.',nrm,afx,(op,tg) => op.k!=='reverse'&&(op.old=tg.slice()));
    amu.forEach=amu.map=function(f) {
        let h=hst(this[RAW],this,_,'arr'); f.length>1&&(h.ix=[]); h.up=$w; h.each=f; h.mnt=hMnt; return h;
    };
    //没找到重新执行一次raw
    ['filter','concat','every','some','includes','indexOf','lastIndexOf'].forEach(k => amu[k]=function(...ag) {
        let tg=this[RAW],ret,raw; sBond(tg); ret=tg[k](...ag);
        return (ret<0||ret===false)&&(raw=ag[0]?.[RAW])&&raw!==ag[0]? (ag[0]=raw,tg[k](...ag)):ret;
    });
};
// useMap：Map 点订阅
export let useMap=() => {
    if(_mmu++) return;
    setMutReplay(mutReplay);
    let mKpu=(tg,k,m=tg[KPU]??=new Map) => m.get(k)??(m.set(k,k=new MSig(tg,k)),k);
    opr(mmu,{
        set(tg,ag,_c,k=ag[0],v=ag[1]) {let s=tg[KPU]?.get(k); s? s.$=v:tg.set(k,v); return tg;},
        delete(tg,ag,_c,k=ag[0]) {let r=tg.delete(k),s=tg[KPU]?.get(k); r&&s&&trig(s); return r;},
        clear(tg) {let kpu=tg[KPU]; tg.clear(); if(kpu) for(let s of kpu.values()) trig(s);},
    },'map.',_,_,(op,tg,ag,k=ag[0]) => op.k==='clear'? op.old=new Map(tg):(op.had=tg.has(k))&&(op.old=tg.get(k)));
    ['get','has'].forEach(k => mmu[k]=function(k2) {let tg=this[RAW]; return $b&&bond(mKpu(tg,k2)),asPrx(tg[k](k2));});
    ['keys','values','entries','forEach'].forEach(k => mmu[k]=function(...ag) {let tg=this[RAW]; sBond(tg); return tg[k](...ag);});
};
export let useMut=() => (useArr(),useMap());
export let useDOM=v => swp=(DMR=v)? _swp:nop as any;

// useReset：整体替换时候复用
export let useReset=() => {
    if(_rst++) return;
    // pair：新旧对位
    let pair=(tg: any[],vs: any[],kf: Fn,c?: HCtx) => {
        let M=tg.length,N=vs.length,i=0,a=M-1,b=N-1,nm: any,map,q,j,k;
        while(i<M&&i<N&&kf(tg[i])===kf(vs[i])) i++;
        c&&(c.st=i,c.d=c.a=0); if(i===M&&i===N) return _;
        nm=Array(N); for(j=0;j<i;j++) nm[j]=j;
        while(a>=i&&b>=i&&kf(tg[a])===kf(vs[b])) nm[b--]=a--;
        if(i>b) return nm;
        if(i>a) {for(j=i;j<=b;j++) nm[j]=-1; return nm;}
        map=new Map(); for(j=i;j<=a;j++) map.set(kf(tg[j]),j);
        for(j=i;j<=b;j++) nm[j]=(q=map.get(k=kf(vs[j])))===_? -1:(map.delete(k),q);
        return nm;
    };
    // patch：字段写回旧壳
    let patch=(a: any,b: any) => {let p=a[PRX]||a,k; for(k in b) a[k]!==b[k]&&(p[k]=b[k]); for(k in a) k in b||(p[k]=_);};
    // hsync：镜像跟走，返回死行
    let hsync=(h: H,nm: num[]) => {
        let N=nm.length,ows=h.res||[],oix=h.ix,ws=h.res=Array(N),ix=oix&&(h.ix=Array(N)),j,o;
        for(j=0;j<N;j++) if((o=nm[j])>=0) {ws[j]=ows[o]; ows[o]=_; ix&&(ix[j]=oix![o]);}
        return ows;
    };
    let d=function(tg,ag,c,vs=ag[0],key=ag[1]) {
        let kf=isFn(key)? key:key? (x => x?.[key]):(x => x),N=vs.length,j=0,o,a,b,out;
        let nm=pair(tg,vs,kf,c); c&&(c.nm=nm);
        if(!nm) {
            for(;j<N;j++) {a=tg[j]; b=vs[j]; if(a===b) continue; isObj(a)&&isObj(b)? patch(a,b):(tg[j]=b,c&&(c.st=Min(c.st,j)));}
            return tg;
        }
        for(out=Array(N);j<N;j++) {b=vs[j]; o=nm[j]; out[j]=o<0? b:(a=tg[o],(a===b||!isObj(a)||!isObj(b))? b:(patch(a,b),a));}
        for(j=0;j<N;j++) tg[j]=out[j]; tg.length=N;
        return tg;
    };
    afx.reset=(h: H,c: HCtx) => {
        let nm=c.nm;
        if(!nm) return;
        let d=hsync(h,nm),ws=h.res,N=ws.length,w,j,{n0=0,n1=N}=h;
        if(h.n1!==_) {
            n0=h.n0=Min(h.n0,Max(0,N-1)); n1=h.n1=Min(n0+h.gp,N);
            for(j=0;j<N;j++) if((w=ws[j])&&(j<n0||j>=n1)) {clr(w); ws[j]=_;}
        }
        reorder(h,d,n0,n1); h.adj? h.adj(h,{st: 0,d: 0,a: 0}):ixr(h,0);
    };
    opr(amu,{reset: d},'arr.',_,afx,(op,tg) => op.old=tg.map((x: any) => isObj(x)? {...x}:x));
};
// reset：整体替换入口
export let reset=(tg: any,vs: any[],key?: any) => {let a=$(tg),p=a?.[PRX],j; if(p?.reset) return p.reset(vs,key); if(a) {for(j=0;j<vs.length;j++) a[j]=vs[j]; a.length=vs.length;} };


