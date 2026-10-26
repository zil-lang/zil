const _=undefined, Jsn=JSON.stringify, oKeys=Object.keys;
const log=(...args: any[]) => (globalThis as any).console?.log(...args), error=(...args: any[]) => (globalThis as any).console?.error(...args);
const isObj=(x: any) => typeof x==='object'&&x, isFn=(x: any) => typeof x==='function';
const toStr=Object.prototype.toString;
export let TOPT={VERBOSE: 0,TLOG: 1,THROW: 1};
export let topt=TOPT; // 兼容旧名
/** 测试进度标题（如 `[场景21] …`）；非断言，默认开 */
export let tlog=(m: string) => {if(TOPT.TLOG) log(m);};
export type Diff={p: string; a: any; b: any;};
export function fmt(v: any,d=3,seen?: WeakMap<object,number>,id=0): string {
    if(v==null) return ''+v;
    let t=typeof v;
    if(t==='string') return v.length>200? Jsn(v.slice(0,200))+'…('+v.length+')':Jsn(v);
    if(t==='number'||t==='boolean'||t==='bigint') return ''+v;
    if(t==='symbol') return v.toString();
    if(t==='function') return `[fn ${v.name||'anon'}]`;
    if(!isObj(v)) return ''+v;
    let s=toStr.call(v);//@ts-ignore
    if(s==='[object Error]') return v.stack||`${v.name||'Error'}: ${v.message}`;
    seen??=new WeakMap();
    if(seen.has(v)) return `[Circular#${seen.get(v)}]`;
    seen.set(v,++id);
    if(d<=0) return s.slice(8,-1);//@ts-ignore
    if(s==='[object Array]') return '['+v.slice(0,20).map((x: any) => fmt(x,d-1,seen,id)).join(', ')+(v.length>20? ', …('+v.length+')':'')+']';
    if(s==='[object Map]') {let m=v as Map<any,any>,a: string[]=[]; for(let [k,x] of m) {a.push(fmt(k,1,seen,id)+': '+fmt(x,d-1,seen,id)); if(a.length>=20) break;} return `Map(${m.size}){`+a.join(', ')+(m.size>20? ', …':'')+'}';}
    if(s==='[object Set]') {let m=v as Set<any>,a: string[]=[]; for(let x of m) {a.push(fmt(x,d-1,seen,id)); if(a.length>=20) break;} return `Set(${m.size}){`+a.join(', ')+(m.size>20? ', …':'')+'}';}
    let ks=oKeys(v),a: string[]=[];
    for(let i=0;i<ks.length&&i<20;i++) {let k=ks[i]; a.push(k+': '+fmt(v[k],d-1,seen,id));}
    return '{'+a.join(', ')+(ks.length>20? ', …('+ks.length+')':'')+'}';
}
/** 首个差异路径；相等返回 undefined */
export function diff(a: any,b: any,p='',seen?: WeakMap<object,any>): Diff|undefined {
    if(a===b) return;
    if(!isObj(a)||!isObj(b)) return {p,a,b};
    let t=toStr.call(a); if(t!==toStr.call(b)) return {p,a,b};
    seen??=new WeakMap();
    if(seen.get(a)===b) return _; seen.set(a,b);
    if(t==='[object Date]') return +a===+b? _:{p,a,b};
    if(t==='[object RegExp]') return ''+a===''+b? _:{p,a,b};//@ts-ignore
    if(t==='[object Error]') {
        for(const key of ['name','message','cause']) {let r=diff(a[key],b[key],p? p+'.'+key:key,seen); if(r) return r;}
    }
    if(t==='[object ArrayBuffer]'||t==='[object DataView]') {
        if(a.byteLength!==b.byteLength) return {p: p+'.byteLength',a: a.byteLength,b: b.byteLength};
        const av=t==='[object ArrayBuffer]'? new Uint8Array(a):new Uint8Array(a.buffer,a.byteOffset,a.byteLength);
        const bv=t==='[object ArrayBuffer]'? new Uint8Array(b):new Uint8Array(b.buffer,b.byteOffset,b.byteLength);
        for(let i=0;i<av.length;i++) if(av[i]!==bv[i]) return {p: p+'['+i+']',a: av[i],b: bv[i]};
        return;
    }
    if(t==='[object Array]') {if(a.length!==b.length) return {p: p+'.length',a: a.length,b: b.length}; for(let i=0;i<a.length;i++) {let r=diff(a[i],b[i],p+'['+i+']',seen); if(r) return r;} return;}
    if(t==='[object Map]'||t==='[object Set]') {
        let ma=a as Map<any,any>&Set<any>,mb=b as Map<any,any>&Set<any>;
        if(ma.size!==mb.size) return {p: p+'.size',a: ma.size,b: mb.size};
        if(t==='[object Set]') {for(let v of ma) if(!mb.has(v)) return {p: p+'.has('+fmt(v,1)+')',a: 1,b: 0}; return;}
        for(let [k,v] of ma) {let pk=p+'.get('+fmt(k,1)+')'; if(!mb.has(k)) return {p: pk,a: v,b: _}; let r=diff(v,mb.get(k),pk,seen); if(r) return r;}
        return;
    }
    let ka=oKeys(a),kb=oKeys(b);
    if(ka.length!==kb.length) return {p: p+'.keys',a: ka,b: kb};
    for(let i=0;i<ka.length;i++) {let k=ka[i]; if(!(k in b)) return {p: p+'.'+k,a: a[k],b: _}; let r=diff(a[k],b[k],p? p+'.'+k:k,seen); if(r) return r;}
}
export let same=(a: any,b: any): boolean => a===b||isObj(a)&&isObj(b)&&!diff(a,b);
/** 失败栈：首个非 __/zst 的 basename:line */
export let loc=(stack?: string,skip=/__\.ts|zst\.ts|[/\\]roam[/\\]check\.ts|node_modules/i): string => {
    try {
        let s=stack||(new Error().stack||'');
        for(let ln of s.split('\n')) {
            if(!ln) continue;
            let m=ln.match(/^(?:\s*at\s+)?(?:.+?\s+\()?(.+?):(\d+):\d+\)?$/);
            if(!m) continue;
            let full=(m[1]||'').replace(/[?#].*$/,'').trim();
            if(skip.test(full)) continue;
            let path=full.replace(/^file:\/\/\/?/,'').replace(/^.*[/\\]/,'');
            if(path&&m[2]) return path+':'+m[2];
        }
    } catch { /* ignore */}
    return '';
};
export let PASS=0,FAIL=0,FAILS: string[]=[];
export let reset=() => {FAILS.length=PASS=FAIL=0;};
export class AssertErr extends Error {constructor(m: string) {super(m); this.name='AssertionError';} }
let fail=(m: string): never|0 => {
    FAIL++; FAILS.push(m); error('  FAIL '+m); if(TOPT.THROW) throw new AssertErr(m); return 0;
};
export let ok=(c: any,m=''): boolean => {
    if(c) {PASS++; TOPT.VERBOSE&&log('  OK '+m); return true;}
    return fail(m+(loc()? ' '+loc():'')) as any;
};
export let eq=(got: any,exp: any,m=''): boolean => {
    if(got===exp||Object.is(got,exp)||same(got,exp)||isFn(exp)&&exp(got)) {PASS++; TOPT.VERBOSE&&log('  OK '+m); return true;}
    let r=isFn(exp)? _:diff(got,exp),at=loc();
    return fail(m+(at? ' '+at:'')+(r?.p? ' @ '+r.p:'')+'\n期望 '+(isFn(exp)? '谓词 ':'')+fmt(exp)+'\n实际 '+fmt(got)) as any;
};
/** esTrue/esFalse=严格布尔；notNull；tobe(a,b,msg)|tobe(a,msg)|tobe(a,b) */
export let esTrue=(v: boolean,m='') => ok(v===true,m);
export let esFalse=(v: boolean,m='') => ok(v===false,m);
export let no=esFalse;
export let notNull=(v: any,m='') => ok(v!=null,m);
export function tobe(a: any,b?: any,msg?: string) {
    return arguments.length>=3? eq(a,b,msg):(typeof b==='string'||arguments.length<2? ok(!!a,b as any):eq(a,b));
}
export let line=(p=PASS,f=FAIL) => `${p} passed, ${f} failed (${p}/${p+f})`;
export let end=(doExit=true) => (log(`\n== ${PASS} passed, ${FAIL} failed (${PASS}/${PASS+FAIL}) ==`),doExit&&(globalThis as any).process&&FAIL&&(globalThis as any).process.exit(1));
