const G=globalThis as any;
export let $real={st: G.setTimeout.bind(G),ct: G.clearTimeout.bind(G),raf: G.requestAnimationFrame?.bind(G),caf: G.cancelAnimationFrame?.bind(G),si: G.setInterval?.bind(G),ci: G.clearInterval?.bind(G)};
/** 假时钟：替换 G 的 setTimeout/raf；Node 侧勿默认装（会打瞎 test 超时，须 try/finally 成对 off） */
export let $clk: {now: () => number; adv: (ms?: number) => Promise<void>; off: () => void;}|undefined;
const clockHosts=new WeakSet<object>();
export function createClock(g: any) {
    if(clockHosts.has(g)) throw new Error('clock host already owned');
    const keys=['setTimeout','clearTimeout','requestAnimationFrame','cancelAnimationFrame'];
    const descriptors=keys.map(key => Object.getOwnPropertyDescriptor(g,key));
    // Refuse non-writable/accessor hosts before replacing even the first timer.
    for(const key of keys) {
        let owner=g,descriptor: PropertyDescriptor|undefined;
        while(owner&&!descriptor) {descriptor=Object.getOwnPropertyDescriptor(owner,key); owner=Object.getPrototypeOf(owner);}
        if(descriptor&&(!('value' in descriptor)||!descriptor.writable)||!Object.hasOwn(g,key)&&!Object.isExtensible(g))
            throw new TypeError(`clock host property is not writable: ${key}`);
    }
    const restore=() => {for(let i=keys.length-1;i>=0;i--) {const d=descriptors[i]; if(d) Object.defineProperty(g,keys[i],d); else delete g[keys[i]];}};
    let active=true;
    let now=0,id=0,q: {t: number; id: number; f: () => void;}[]=[];
    try {
    g.setTimeout=(f: any,ms=0,...a: any[]) => (q.push({t: now+Math.max(0,+ms||0),id: ++id,f: () => f(...a)}),id);
    g.clearTimeout=(i: any) => {let k=q.findIndex(x => x.id===i); k>=0&&q.splice(k,1);};
    g.requestAnimationFrame=(f: any) => g.setTimeout(() => f(now),16);
    g.cancelAnimationFrame=g.clearTimeout;
    clockHosts.add(g);
    } catch(error) {restore(); throw error;}
    return {
        now: () => now,
        adv: async (ms=1e4) => {
            if(!active) throw new Error('clock is disposed');
            if(!Number.isFinite(ms)||ms<0) throw new RangeError('finite nonnegative clock advance required');
            let end=now+ms;
            for(let n=0;n<1e5&&q.length;n++) {
                q.sort((a,b) => a.t-b.t||a.id-b.id);
                if(q[0].t>end) break;
                let j=q.shift()!; now=j.t; j.f();
                // async/await 链常需多拍 microtask；顺带吃到本拍新挂上的同刻度 timer
                for(let k=0;k<32;k++) await Promise.resolve();
            }
            if(q.some(job => job.t<=end)) throw new Error('clock task budget exceeded');
            now=end;
        },
        off: () => {if(!active) return; active=false; q.length=0; clockHosts.delete(g); restore();},
    };
}

/** Legacy singleton; new sessions create and own their own host clock. */
export function clk(g: any=G) {
    if($clk) return $clk;
    let clock=createClock(g),off=clock.off;
    clock.off=() => {off(); if($clk===clock) $clk=undefined;};
    return $clk=clock;
}
