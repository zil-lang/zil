import {$real} from '../clock';
let _req=(import.meta as any).require;
if(!_req&&!(globalThis as any).document&&(globalThis as any).process?.versions?.node) {
    const moduleId='node:module';
    _req=(await import(/* @vite-ignore */ moduleId)).createRequire(import.meta.url);
}

let _ntm: any=(globalThis as any).document||!_req? {}:_req('node:test');
export let {describe,before,after,beforeEach,afterEach}=_ntm;
let _nt=_ntm.test;
// —— node:test 胶水：超时用 $real.st；test 只负责正确性测试——
export const MS=5000;
function ntWrap(name: string,fn: (...a: any[]) => any,ms: number) {
    return async (...a: any[]) => {
        let tm: any,done=0;
        let p=Promise.resolve().then(() => fn(...a));
        p.catch(() => {});
        try {
            await Promise.race([
                p.then(v => {done=1; return v;}),
                new Promise((_,rj) => {tm=$real.st(() => rj(new Error(`timeout ${ms}ms: ${name}`)),ms);}),
            ]);
        } finally {$real.ct(tm);}
    };
}
/** test(name, fn)|test(name, fn, ms)|test(name, opts, fn) → node:test */
export function test(name: string,a?: any,b?: any,c?: any) {
    if(typeof a==='number') throw new Error('test expects a function or options; use bm for benchmarks');
    if(typeof a==='function') return _nt(name,{timeout: (typeof b==='number'? b:5000)+50},ntWrap(name,a,typeof b==='number'? b:5000));
    let ms=a?.timeout??5000;
    return _nt(name,{...a,timeout: ms+50},ntWrap(name,b,ms));
}
(test as any).skip=(name: string,a?: any,b?: any,c?: any) => {
    if(typeof a==='number') throw new Error('test.skip expects a function or options');
    if(typeof a==='function') return (_nt as any).skip(name,{timeout: (typeof b==='number'? b:5000)+50},ntWrap(name,a,typeof b==='number'? b:5000));
    let ms=a?.timeout??5000;
    return (_nt as any).skip(name,{...a,timeout: ms+50},ntWrap(name,b,ms));
};
export let it=test;
