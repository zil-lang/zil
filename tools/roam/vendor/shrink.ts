import {cloneCase,type Action,type RoamCase,type RunResult} from './core';

export interface ShrinkOptions {
    maxRuns?: number;
    budgetMs?: number;
    signal?: {readonly aborted: boolean;};
    now?: () => number;
    /** Each yielded action must be strictly simpler; identity and cycles are ignored. */
    simplify?(action: Action): Iterable<Action>;
}
export interface ShrinkResult {
    original: RoamCase;
    reduced: RoamCase;
    result: RunResult;
    runs: number;
    stopped: 'complete'|'budget'|'aborted'|'not-reproduced';
}
/** Independent shrink worker. Invalid paths and different failures never replace evidence. */
export async function shrink(original: RoamCase,replay: (testCase: RoamCase) => Promise<RunResult>,options: ShrinkOptions={}): Promise<ShrinkResult> {
    const source=cloneCase(original),now=options.now??Date.now,start=now();
    let reduced=cloneCase(source),runs=0,result: RunResult={status: 'incomplete',steps: 0,issues: [],reason: 'shrink-budget'};
    let stopped: ShrinkResult['stopped']='complete';
    const budget=() => {
        if(options.signal?.aborted) {stopped='aborted'; return false;}
        if(runs>=(options.maxRuns??1000)||(options.budgetMs!==undefined&&now()-start>=options.budgetMs)) {stopped='budget'; return false;}
        return true;
    };
    const run=async (c: RoamCase): Promise<RunResult> => {
        runs++;
        try {return await replay(cloneCase(c));}
        catch(e) {return {status: 'error',steps: 0,issues: [{phase: 'replay',name: e instanceof Error? e.name:'ThrownValue',message: e instanceof Error? e.message:String(e)}]};}
    };
    if(!budget()) return {original: source,reduced,result,runs,stopped};
    result=await run(reduced);
    if(result.status!=='fail'||!result.failure?.propertyId) return {original: source,reduced,result,runs,stopped: 'not-reproduced'};
    const propertyId=result.failure.propertyId;
    const seen=new Set([JSON.stringify(reduced)]);
    const accept=async (candidate: RoamCase): Promise<boolean> => {
        try {candidate=cloneCase(candidate);} catch {return false;}
        const key=JSON.stringify(candidate);
        if(seen.has(key)) return false;
        seen.add(key);
        const next=await run(candidate);
        if(next.status!=='fail'||next.failure?.propertyId!==propertyId) return false;
        reduced=cloneCase(candidate); result=next; return true;
    };
    // Delta debugging removes blocks, then reaches a fixed point for single deletions.
    let size=Math.max(1,Math.ceil(reduced.steps.length/2));
    while(reduced.steps.length&&budget()) {
        let changed=false;
        for(let i=0;i<reduced.steps.length&&budget();i+=size) {
            const candidate=cloneCase(reduced); candidate.steps.splice(i,size);
            if(await accept(candidate)) {changed=true; break;}
        }
        if(changed) {size=Math.min(size,Math.max(1,reduced.steps.length)); continue;}
        if(size===1) break;
        size=Math.max(1,Math.floor(size/2));
    }
    if(options.simplify&&budget()) {
        for(let i=0;i<reduced.steps.length&&budget();i++) {
            let changed=true;
            while(changed&&budget()) {
                changed=false;
                for(const action of options.simplify(reduced.steps[i])) {
                    if(!budget()) break;
                    const candidate=cloneCase(reduced); candidate.steps[i]=action;
                    if(await accept(candidate)) {changed=true; break;}
                }
            }
        }
    }
    budget(); // Include the final replay, even when no candidates remain to trigger another check.
    // Complete describes this strategy's search, never a proof of globally shortest input.
    // result always belongs to reduced, never a rejected candidate or stale original failure.
    return {original: source,reduced,result,runs,stopped};
}
