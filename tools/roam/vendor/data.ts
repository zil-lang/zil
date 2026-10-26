/** 可复现 PRNG（mulberry32） */
export let seed=(a: number): (t?: any) => number =>
    t => (a=a+0x6D2B79F5|0,t=Math.imul(a^a>>>15,1|a),t+=Math.imul(t^t>>>7,61|t)^t,((t^t>>>14)>>>0)/4294967296);
export type Rng=() => number;
/** seeded 组合子：gInt/gOne/gArr/gRec */
export let gInt=(a=0,b=9) => (r: Rng) => a+((r()*(b-a+1))|0);
export let gOne=<T>(...xs: T[]) => (r: Rng) => xs[(r()*xs.length)|0];
export let gArr=<T>(g: (r: Rng) => T,n: number|((r: Rng) => number)=gInt(0,5)) => (r: Rng) => Array.from({length: typeof n==='function'? n(r):n},() => g(r));
export let gRec=<T extends Record<string,(r: Rng) => any>>(o: T) => (r: Rng) => {let v: any={}; for(let k in o) v[k]=o[k](r); return v as {[K in keyof T]: ReturnType<T[K]>;};};
export function shuffle<T>(a: T[],rdm: () => number=Math.random): T[] {a=a.slice(); for(let i=a.length-1;i>0;i--) {let j=(rdm()*(i+1))|0;[a[i],a[j]]=[a[j],a[i]];} return a;}
/** 测数据：ss=长度或各维长度；fv=常量或 (depth,i)=>item */
export function genArr(ss: any,fv: any) {
    let f=typeof fv==='function'? fv:() => fv; if(!ss[1]) {let n=ss[0]||ss,arr=[],i=0; for(;i<n;i++)arr[i]=f(1,i); return arr;}
    let gen=(d: number,i=0): any => {if(d>=ss.length) return f(d+1,i); let arr=[]; for(;i<ss[d];i++) arr[i]=gen(d+1,i); return arr;};
    return gen(0);
}
/** 测数据：广度 k、深度 d 的嵌套对象 */
export function genObj(k=5,d=1): any {
    if(d===0) return Math.random();
    let obj: Record<string,any>={},i=0; for(;i<k;i++) obj[`key-`+i]=genObj(k,d-1);
    return obj;
}
/** 包装 fn，记录 calls；reset 清空 */
export function spy<F extends (...a: any[]) => any>(f: F=(() => {}) as any) {
    let fn: any=(...ag: any[]) => (fn.calls.push(ag),f(...ag));
    fn.calls=[]; fn.reset=() => {fn.calls=[];};
    return fn as F&{calls: any[][]; reset: () => void;};
}
/** 二分最小仍失败 size：bad(sz) 真=失败；默认 lo=1 */
export let minimize=(hi: number,bad: (sz: number) => any,lo=1): {size: number; fail: any;} => {
    let fail=bad(hi); if(!fail) return {size: hi,fail};
    let best=hi;
    while(lo<best) {let mid=(lo+best)>>1,r=bad(mid); if(r) {fail=r; best=mid;} else lo=mid+1;}
    return {size: best,fail};
};
