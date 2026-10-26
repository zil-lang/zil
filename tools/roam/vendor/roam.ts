import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {execFile} from 'node:child_process';
import {createHash} from 'node:crypto';
import {FileStore,failureKey,type Lease} from './store';
import {supervise,generateCase} from './worker';
import {freezeDriver} from './snapshot';
import {type RoamCase,type RunResult,type Json} from './core';
import {shrink,type ShrinkResult} from './shrink';

export type Target={driver: string; build: string;}|{snapshot: string; build?: string;};
export interface RoamOptions {
    root?: string;
    target?: Target;
    runtime?: 'node'|'bun';
    timeoutMs?: number;
    capacity?: number;
    leaseMs?: number;
    worker?: string;
}
export interface ExploreOptions {cases?: number; steps?: number; seed?: number; budgetMs?: number; case?: RoamCase;}
export interface ReplayOptions {id?: string; case?: RoamCase; repeat?: number; budgetMs?: number;}
export interface ReduceOptions {id?: string; maxRuns?: number; budgetMs?: number;}
export type Mode='gate'|'explore'|'soak'|'replay'|'reduce';
export interface ExploreReport {mode: Mode; counts: Record<string,number>; completed: number; actions: number; failures: string[]; stop: string; elapsedMs: number;}
export interface GenerationFailure {mode: Mode; generationFailure: string; message: string;}
export interface Unclaimed {mode: Mode; claimed: false;}
export interface ReplayReport {mode: Mode; id: string|null; classification: 'stable'|'intermittent'|'not-reproduced'|'infrastructure-or-other'; reproduced: number; attempts: number; results: RunResult[]; failures: string[];}
export interface ReduceReport {mode: Mode; id: string; counts: Record<string,number>; completed: number; reduced: ShrinkResult;}
function positive(value: number|undefined,fallback: number,name: string) {const n=value??fallback; if(!Number.isSafeInteger(n)||n<=0) throw new Error(`${name} must be a positive integer`); return n;}
async function resolveRuntime(kind: 'node'|'bun',command: string) {
    const code="console.log(JSON.stringify({executable:process.execPath,version:typeof Bun==='undefined'?process.version:Bun.version,kind:typeof Bun==='undefined'?'node':'bun'}))";
    return new Promise<{kind: 'node'|'bun'; executable: string; version: string;}>((resolve,reject) => {
        execFile(command,['-e',code],{timeout: 2000,windowsHide: true,maxBuffer: 65536},(error,stdout) => {
            if(error) return reject(error);
            try {const info=JSON.parse(stdout); if(info.kind!==kind||typeof info.executable!=='string'||typeof info.version!=='string') throw new Error('runtime identity mismatch'); resolve(info);} catch(e) {reject(e);}
        });
    });
}
/** Shared configuration only. Leases, budgets and execution state belong to each call. */
export class Roam {
    readonly store: FileStore;
    private readonly options: Readonly<RoamOptions>;
    constructor(options: RoamOptions={}) {
        this.options={...options,target: options.target? {...options.target}:undefined};
        if(options.runtime&&options.runtime!=='node'&&options.runtime!=='bun') throw new Error('runtime must be node or bun');
        positive(options.timeoutMs,10000,'timeout'); positive(options.leaseMs,60000,'lease');
        this.store=new FileStore(options.root??'test-results/roam',options.capacity===undefined? Infinity:positive(options.capacity,1,'capacity'));
    }
    explore(options: ExploreOptions={}): Promise<ExploreReport|GenerationFailure> {return this.run('explore',options);}
    gate(options: ExploreOptions={}): Promise<ExploreReport|GenerationFailure> {return this.run('gate',options);}
    soak(options: Omit<ExploreOptions,'cases'>={}): Promise<ExploreReport|GenerationFailure> {return this.run('soak',options);}
    replay(options: ReplayOptions={}): Promise<ReplayReport|Unclaimed> {return this.run('replay',options);}
    reduce(options: ReduceOptions={}): Promise<ReduceReport|Unclaimed> {return this.run('reduce',options);}
    private run(mode: 'explore'|'gate'|'soak',request: ExploreOptions): Promise<ExploreReport|GenerationFailure>;
    private run(mode: 'replay',request: ReplayOptions): Promise<ReplayReport|Unclaimed>;
    private run(mode: 'reduce',request: ReduceOptions): Promise<ReduceReport|Unclaimed>;
    private async run(mode: Mode,request: ExploreOptions&ReplayOptions&ReduceOptions): Promise<ExploreReport|GenerationFailure|ReplayReport|ReduceReport|Unclaimed> {
        const opt=this.options,store=this.store;
        const timeout=positive(opt.timeoutMs,10000,'timeout'),budget=positive(request.budgetMs,60000,'budget');
        const count=positive(mode==='reduce'? request.maxRuns:request.cases,100,'cases');
        const steps=positive(request.steps,100,'steps'),seed=positive(request.seed,1,'seed'),repeat=positive(request.repeat,1,'repeat'),leaseMs=positive(opt.leaseMs,60000,'lease');
        const queued=mode==='replay'||mode==='reduce';
        let lease: Lease|undefined=queued&&!request.id&&!request.case? store.claim(mode,opt.worker??`pid-${process.pid}`,leaseMs):undefined;
        const id=request.id??lease?.id;
        if(queued&&!id&&!request.case) return {mode,claimed: false};
        let leaseError: unknown,fatalError: unknown;
        const leaseHeartbeat=lease? setInterval(() => {try {lease=store.renew(lease!,leaseMs);} catch(error) {leaseError=error;} },Math.max(1,Math.floor(leaseMs/3))):undefined;
        try {
            const original=id? store.read(id):undefined,target=opt.target;
            const sourceHash=target&&'snapshot' in target? target.snapshot:!target? original?.environment?.entryHash:undefined;
            let driverPath: string;
            if(target&&'driver' in target) {
                if(!target.build) throw new Error('build required when compiling a new driver');
                driverPath=await freezeDriver(resolve(target.driver),store.root);
            } else {
                if(typeof sourceHash!=='string'||!/^[a-f0-9]{64}$/.test(sourceHash)) throw new Error('driver or a valid stored/explicit snapshot is required');
                driverPath=resolve(store.root,'.snapshots',sourceHash,'driver.mjs');
                if(createHash('sha256').update(readFileSync(driverPath)).digest('hex')!==sourceHash) throw new Error('stored snapshot hash mismatch');
            }
            const entryHash=createHash('sha256').update(readFileSync(driverPath)).digest('hex');
            const build=target?.build??original?.environment?.build??entryHash;
            const kind=opt.runtime??original?.environment?.runtime?.kind??((globalThis as any).Bun? 'bun':'node');
            if(kind!=='node'&&kind!=='bun') throw new Error('invalid stored runtime');
            const command=opt.runtime? kind:original?.environment?.runtime?.executable??process.execPath;
            const runtime=await resolveRuntime(kind,command);
            if(original&&!opt.runtime&&!(target&&'driver' in target)) {
                const expected=original.environment?.runtime;
                if(!expected||typeof expected.version!=='string') throw new Error('stored runtime identity is missing a version');
                if(expected.version!==runtime.version) throw new Error(`runtime version mismatch: stored ${expected.version}, actual ${runtime.version}; use explicit --runtime for a cross-runtime regression`);
            }
            const environment: Json={build,entryHash,runtime,driver: driverPath,mode};
            const start=Date.now(),counts: Record<string,number>={},failures: string[]=[];
            let completed=0,actionCount=0;
            const run=async (trace: RoamCase,persist=true) => {
                try {
                    if(fatalError) throw fatalError;
                    if(leaseError) throw leaseError;
                    if(createHash('sha256').update(readFileSync(driverPath)).digest('hex')!==entryHash) throw new Error('driver snapshot changed during run');
                    const remaining=budget-(Date.now()-start); if(remaining<=0) throw new Error('total execution budget exhausted');
                    let heartbeatError: unknown;
                    const heartbeat=setInterval(() => {try {store.statistics({mode,build,completed,actions: actionCount,counts,active: true,heartbeat: Date.now()});} catch(error) {heartbeatError=error;} },1000);
                    let output;
                    try {output=await supervise({command: runtime.executable,args: [driverPath],trace,required: trace.requires,timeoutMs: Math.min(timeout,remaining),store: persist? store:undefined,environment});}
                    finally {clearInterval(heartbeat);}
                    if(output.result.issues.some(x => x.phase==='host-cleanup'&&x.name==='TreeCleanupError')) {
                        const evidence=output.evidenceId??store.publish(trace,output.result,environment);
                        fatalError=new Error(`process tree cleanup failed; exploration stopped, evidence ${evidence}`);
                        throw fatalError;
                    }
                    if(heartbeatError) throw heartbeatError;
                    counts[output.result.status]=(counts[output.result.status]??0)+1; completed++; actionCount+=output.result.steps;
                    if(output.evidenceId) failures.push(output.evidenceId);
                    store.statistics({mode,build,completed,actions: actionCount,counts,elapsedMs: Date.now()-start,heartbeat: Date.now()});
                    return output.result;
                } catch(error) {fatalError=error; throw error;}
            };
            if(queued) {
                const trace=original?.trace??request.case!;
                if(mode==='reduce') {
                    if(!id) throw new Error('reduce requires stored evidence');
                    const reduced=await shrink(trace,c => run(c,false),{maxRuns: count,budgetMs: budget,signal: {get aborted() {return !!fatalError||!!leaseError;} }});
                    if(fatalError) throw fatalError;
                    if(leaseError) throw leaseError;
                    store.reduced(id,reduced.reduced,reduced.result);
                    if(lease) store.complete(lease,{status: reduced.result.status,stopped: reduced.stopped,runs: reduced.runs,entryHash});
                    return {mode,id,counts,completed,reduced};
                }
                const results: RunResult[]=[];
                for(let i=0;i<repeat;i++) results.push(await run(trace,!original));
                if(leaseError) throw leaseError;
                const expectedFailure=original? failureKey(original.result):undefined;
                const reproduced=results.filter(r => expectedFailure? failureKey(r)===expectedFailure:r.status==='fail').length;
                const classification=reproduced===repeat? 'stable':reproduced? 'intermittent':results.every(r => r.status==='pass')? 'not-reproduced':'infrastructure-or-other';
                if(lease) store.complete(lease,{classification,reproduced,attempts: repeat,entryHash});
                return {mode,id: id??null,classification,reproduced,attempts: repeat,results,failures};
            }
            let stop='case-budget';
            for(let index=0;index<(mode==='soak'? 1:count);index++) {
                if(Date.now()-start>=budget) {stop='time-budget'; break;}
                let trace: RoamCase;
                if(request.case) trace=request.case;
                else {
                    try {trace=await generateCase({command: runtime.executable,args: [driverPath,'generate'],seed: seed+index,steps,timeoutMs: Math.min(timeout,Math.max(1,budget-(Date.now()-start)))});}
                    catch(error) {
                        const detail=error instanceof Error? error.message:String(error);
                        const id=store.publish({format: 'roam/1',contract: 'roam/generator/1',steps: [],seed: seed+index,meta: {requestedSteps: steps}},{status: 'error',steps: 0,issues: [{phase: 'generate',name: 'GeneratorError',message: detail},...((error as {cleanupError?: Error;})?.cleanupError? [{phase: 'host-cleanup',name: 'TreeCleanupError',message: (error as {cleanupError: Error;}).cleanupError.message}]:[])]},environment);
                        return {mode,generationFailure: id,message: detail};
                    }
                }
                if(Date.now()-start>=budget) {stop='time-budget'; break;}
                const result=await run(trace);
                if(mode==='gate'&&result.status!=='pass') {stop='gate-failure'; break;}
            }
            return {mode,counts,completed,actions: actionCount,failures,stop,elapsedMs: Date.now()-start};
        } finally {if(leaseHeartbeat) clearInterval(leaseHeartbeat);}
    }
}
