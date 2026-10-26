const log=console.log, error=console.error;
// 以下用于浏览器展示目前仅用于备份，逐渐废弃
// —— 浏览器 cases 跑者：add 登记 / run 前缀批跑（断言抛错即案例失败）；汇总经 onZst 外露（mountZst 右下角 / PW 读 window） ——
let CS: [string,((...a: any[]) => any)][]=[], RP={pass: 0,fail: 0,errs: [] as string[]};
export let add=(name: string,f: ((...a: any[]) => any)) => {CS.push([name,f]);};
export let onZst: ((r: typeof RP) => void)|undefined;
export let run=async (pre='') => {
    for(let [n,f] of CS) {
        if(pre&&!n.startsWith(pre)) continue;
        try {await f(); RP.pass++;}
        catch(e: any) {RP.fail++; RP.errs.push(`${n}: ${e?.stack||e?.message||e}`); error('FAIL '+n);}
    }
    log(`=== 总计:${RP.pass+RP.fail}, 通过:${RP.pass}, 失败:${RP.fail} ===`);
    onZst?.(RP);
    return RP;
};
/** 右下角报告 UI（demo/演示页用；PW 直接 waitForFunction 读完成标记即可，勿依赖本 UI） */
export function mountZst(id: any='zst') {
    if(typeof document==='undefined') return;
    let tg: any=typeof id==='string'? document.getElementById(id):id;
    if(!tg) {
        tg=document.createElement('pre');
        tg.id=typeof id==='string'? id:'zst';
        tg.style.cssText='position:fixed;right:8px;bottom:8px;z-index:99999;max-width:45vw;max-height:45vh;overflow:auto;white-space:pre-wrap;padding:8px 10px;border:1px solid #333;background:#111;color:#ddd;font:12px/1.4 ui-monospace,Menlo,Consolas,monospace;';
        document.body.appendChild(tg);
    }
    onZst=r => {tg.textContent=`总计:${r.pass+r.fail} 通过:${r.pass} 失败:${r.fail}`+(r.errs.length? '\n'+r.errs.join('\n'):'');};
    return tg;
}
