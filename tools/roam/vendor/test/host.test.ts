import { test, expect } from 'bun:test';
import { mkdtempSync, rmSync, readFileSync, readdirSync, writeFileSync, existsSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {FileStore, failureKey} from '../store';
import {supervise, generateCase} from '../worker';
import type { RoamCase, RunResult } from '../core';
const trace: RoamCase = { format: 'roam/1', contract: 'host/1', steps: [{ op: 'tick' }] };
const failure: RunResult = { status: 'fail', steps: 1, failure: { propertyId: 'test', message: 'failure' }, issues: [] };
function temp() { return mkdtempSync(join(tmpdir(), 'roam-host-')); }
const worker = join(import.meta.dir, 'fixtures', 'worker.ts');

test('evidence publication, immutable original, independent reduced and capacity failure', () => {
    const root = temp(); try {
        const store = new FileStore(root), id = store.publish(trace, failure, { build: 'fixed' });
        expect(store.list()).toEqual([id]); expect(store.read(id).trace).toEqual(trace);
        const original = readFileSync(join(root, id, 'case.json'), 'utf8');
        store.reduced(id, { ...trace, steps: [] }, failure);
        expect(readFileSync(join(root, id, 'case.json'), 'utf8')).toBe(original);
        expect(readdirSync(join(root, id)).some(x => x.startsWith('reduced-'))).toBe(true);
        expect(() => new FileStore(root, 1).publish(trace, failure)).toThrow('capacity');
        expect(store.list()).toHaveLength(1);
    } finally { rmSync(root, { recursive: true }); }
});
test('leases exclude peers, recover expired owner, reject stale completion', async () => {
    const root = temp(); try {
        const store = new FileStore(root); store.publish(trace, failure);
        const first = store.claim('confirm', 'one', 15)!; expect(first).toBeDefined();
        expect(store.claim('confirm', 'two')).toBeUndefined();
        await new Promise(r => setTimeout(r, 25));
        const second = store.claim('confirm', 'two')!; expect(second.token).not.toBe(first.token);
        expect(() => store.complete(first, { ok: true })).toThrow('lease lost');
        store.complete(second, { reproduced: false }); expect(store.claim('confirm', 'three')).toBeUndefined();
        expect(store.read(second.id).result).toEqual(failure);
        expect(store.claim('reduce', 'four')).toBeDefined();
    } finally { rmSync(root, { recursive: true }); }
});
test('real worker handshakes, journals before execution, and keeps failures', async () => {
    const root = temp(); try {
        const store = new FileStore(root);
        const pass = await supervise({ command: process.execPath, args: [worker, 'pass'], trace, timeoutMs: 3000, store, required: ['fixture'] });
        expect(pass.result.status).toBe('pass'); expect(pass.journal).toHaveLength(3); expect(store.pending()).toHaveLength(0);
        const fail = await supervise({ command: process.execPath, args: [worker, 'fail'], trace, timeoutMs: 3000, store });
        expect(fail.result.status).toBe('fail'); expect(fail.evidenceId).toBeDefined();
        const history = readFileSync(join(root, fail.evidenceId!, 'attempt', 'history.jsonl'), 'utf8'); expect(history).toContain('"type":"before"');
        const unsupported = await supervise({ command: process.execPath, args: [worker, 'pass'], trace, timeoutMs: 3000, required: ['missing'] }); expect(unsupported.result.status).toBe('unsupported');
    } finally { rmSync(root, { recursive: true }); }
});
test('crash and hung worker persist complete input and pending action; spawn errors settle', async () => {
    const root = temp(); try {
        const store = new FileStore(root);
        for (const mode of ['crash', 'hang', 'malformed', 'spawnerror', 'nohello']) {
            const output = await supervise({ command: process.execPath, args: [worker, mode], trace, timeoutMs: mode === 'hang' || mode === 'nohello' ? 500 : 3000, store });
            expect(output.result.status).toBe('error'); expect(output.evidenceId).toBeDefined();
            expect(store.read(output.evidenceId!).trace).toEqual(trace);
            if (mode === 'hang' || mode === 'crash') expect(output.journal[0]).toContain('before');
        }
        const bad = await supervise({ command: join(root, 'does-not-exist'), trace, timeoutMs: 1000 }); expect(bad.result.status).toBe('error');
        expect(store.pending()).toHaveLength(0);
    } finally { rmSync(root, { recursive: true }); }
});

// CLI integration runs real child workers and only writes inside a disposable evidence root.
test('CLI explores after failure, gates, claims replay and reduces original independently', async () => {
    const root = temp();
    const cli = join(import.meta.dir, '..', 'cli.ts'), driver = join(import.meta.dir, 'fixtures', 'counter.ts');
    const invoke = async (...args: string[]) => {
        const { spawn } = await import('node:child_process');
        return new Promise<{ code: number | null; stdout: string; stderr: string }>((resolve, reject) => {
            const proc = spawn(process.execPath, [cli, ...args, '--driver', driver, '--build', 'fixture-v1', '--root', root], { windowsHide: true });
            let stdout = '', stderr = ''; const timer = setTimeout(() => { proc.kill(); reject(new Error('CLI test timeout')); }, 12000);
            proc.stdout.on('data', x => stdout += x); proc.stderr.on('data', x => stderr += x); proc.once('error', reject);
            proc.once('close', code => { clearTimeout(timer); resolve({ code, stdout, stderr }); });
        });
    };
    try {
        const explore = await invoke('explore', '--cases', '3', '--steps', '3', '--seed', '12');
        expect(explore.code).toBe(0); const summary = JSON.parse(explore.stdout.trim()); expect(summary.completed).toBe(3); expect(summary.counts.fail).toBe(1); expect(summary.counts.pass).toBe(2);
        const store = new FileStore(root), id = store.list()[0], original = readFileSync(join(root, id, 'case.json'), 'utf8');
        const replay = await invoke('replay', '--repeat', '2'); expect(replay.code).toBe(1); expect(JSON.parse(replay.stdout.trim()).classification).toBe('stable');
        expect(store.list()).toHaveLength(1); expect(existsSync(join(root, id, 'replay.done.json'))).toBe(true);
        const reduced = await invoke('reduce', '--cases', '20'); expect(reduced.code).toBe(0); expect(readdirSync(join(root, id)).some(x => x.startsWith('reduced-'))).toBe(true);
        expect(readFileSync(join(root, id, 'case.json'), 'utf8')).toBe(original);
        const gate = await invoke('gate', '--cases', '3', '--steps', '3', '--seed', '13'); expect(gate.code).toBe(1); expect(JSON.parse(gate.stdout.trim()).completed).toBe(1);
        const soak = await invoke('soak', '--steps', '25'); expect(soak.code).toBe(0); expect(JSON.parse(soak.stdout.trim()).actions).toBe(25);
    } finally { rmSync(root, { recursive: true }); }
}, 30000);


test('incomplete directories are invisible and explicit recovery retains raw journal', () => {
    const root = temp(); try {
        const store = new FileStore(root), pending = join(root, '.run-dead'); mkdirSync(pending);
        writeFileSync(join(pending, 'case.json'), JSON.stringify(trace)); writeFileSync(join(pending, 'history.jsonl'), '{"type":"before","step":0}\n');
        expect(store.list()).toHaveLength(0); expect(store.pending()).toEqual(['.run-dead']);
        const recovered = store.recoverPending('.run-dead'); expect(store.list()).toEqual([recovered]);
        expect(store.read(recovered).result.status).toBe('error'); expect(readFileSync(join(root, recovered, 'recovered', 'history.jsonl'), 'utf8')).toContain('before');
        expect(store.pending()).toHaveLength(0);
    } finally { rmSync(root, { recursive: true }); }
});

test('simultaneous independent queue processes acquire a case at most once', async () => {
    const root = temp(); try {
        const store = new FileStore(root); store.publish(trace, failure);
        const { spawn } = await import('node:child_process');
        const claims = await Promise.all(Array.from({ length: 4 }, (_, i) => new Promise<string>((resolve, reject) => {
            const child = spawn(process.execPath, [join(import.meta.dir, 'fixtures', 'claim.ts'), root, String(i)], { windowsHide: true });
            let out = '', err = ''; const timer = setTimeout(() => { child.kill(); reject(new Error('claim process timeout')); }, 5000);
            child.stdout.on('data', x => out += x); child.stderr.on('data', x => err += x); child.once('error', reject);
            child.once('close', code => { clearTimeout(timer); code === 0 ? resolve(out.trim()) : reject(new Error(err)); });
        })));
        expect(claims.filter(x => x !== 'none')).toHaveLength(1);
    } finally { rmSync(root, { recursive: true }); }
});


test('frozen bundle retains relative dependencies after working source changes', async () => {
    const root = temp(); try {
        const { freezeDriver } = await import('../snapshot');
        const dependency = join(root, 'dependency.ts'), entry = join(root, 'domain.ts');
        writeFileSync(dependency, 'export const delta=1;');
        writeFileSync(entry, `import {delta} from './dependency';
export const spec={contract:'snapshot/1',init:()=>0,step:(m)=>m+1,check:(m,observed)=>{if(m!==observed)throw new Error('changed dependency');}};
export const driver={init:()=>({v:0}),step:r=>{r.v+=delta},observe:r=>r.v};`);
        const snapshot = await freezeDriver(entry, root);
        expect(await freezeDriver(entry, root)).toBe(snapshot);
        writeFileSync(dependency, 'export const delta=999;');
        writeFileSync(entry, 'throw new Error("source entry changed");');
        const output = await supervise({ command: process.execPath, args: [snapshot], trace: { format: 'roam/1', contract: 'snapshot/1', steps: [{ op: 'add' }] }, timeoutMs: 3000 });
        expect(output.result.status).toBe('pass'); expect(output.journal).toHaveLength(3);
    } finally { rmSync(root, { recursive: true }); }
});


test('freeze rejects unresolved dynamic imports instead of reading mutable files at run time', async () => {
    const root = temp(); try {
        const { freezeDriver } = await import('../snapshot'), entry = join(root, 'dynamic.ts');
        writeFileSync(entry, `export const spec={contract:'dynamic/1'};export const driver={};export async function generate(){return import(globalThis.runtimePath)}`);
        await expect(freezeDriver(entry, root)).rejects.toThrow('dynamic module loading');
    } finally { rmSync(root, { recursive: true }); }
});


test('crashed capacity owner is recoverable only through explicit offline token recovery', async () => {
    const root = temp(); try {
        const store = new FileStore(root, 1000000), { spawn } = await import('node:child_process');
        const code = await new Promise<number | null>((resolve, reject) => { const child = spawn(process.execPath, [join(import.meta.dir, 'fixtures', 'capacity-crash.ts'), root], { windowsHide: true }); child.once('error', reject); child.once('close', resolve); });
        expect(code).toBe(23); const lock = store.capacityLock()!; expect(lock.owner?.pid).toBeGreaterThan(0);
        expect(() => store.publish(trace, failure)).toThrow();
        expect(() => store.recoverCapacityLock(lock.token, false)).toThrow('all writers stopped');
        expect(() => store.recoverCapacityLock('wrong-token', true)).toThrow('token changed');
        expect(store.recoverCapacityLock(lock.token, true)).toBe(true); expect(store.capacityLock()).toBeUndefined();
        const id = store.publish(trace, failure); expect(store.read(id).trace).toEqual(trace);
        // A live process is never declared stale by age or an operator assertion.
        writeFileSync(join(root, '.capacity.lock'), JSON.stringify({ pid: process.pid, nonce: 'live', created: 0 }));
        expect(() => store.recoverCapacityLock(store.capacityLock()!.token, true)).toThrow('is alive');
    } finally { rmSync(root, { recursive: true }); }
});

test('non-protocol stdout is a reproducible ProtocolError with raw evidence', async () => {
    const root = temp(); try {
        const store = new FileStore(root);
        for (let i = 0; i < 2; i++) {
            const run = await supervise({ command: process.execPath, args: [worker, 'malformed'], trace, timeoutMs: 3000, store });
            expect(run.result.status).toBe('error'); expect(run.result.issues[0].name).toBe('ProtocolError');
            expect(run.result.issues[0].message).toContain('stdout protocol error');
            const history = readFileSync(join(root, run.evidenceId!, 'attempt', 'history.jsonl'), 'utf8'); expect(history).toContain('not json');
        }
    } finally { rmSync(root, { recursive: true }); }
});


test('mutex recovery checks owner/token and never steals an old live-owner lock', async () => {
    const root=temp(); try {
        const store=new FileStore(root), id=store.publish(trace,failure), {spawn}=await import('node:child_process');
        const code=await new Promise<number|null>((resolve,reject)=>{const child=spawn(process.execPath,[join(import.meta.dir,'fixtures','mutex-crash.ts'),root],{windowsHide:true});child.once('error',reject);child.once('close',resolve);});
        expect(code).toBe(24);const lock=store.mutexLock(id)!;expect(lock.owner?.pid).toBeGreaterThan(0);
        expect(()=>store.recoverMutex(id,lock.token,false)).toThrow('all writers stopped');expect(()=>store.recoverMutex(id,'changed',true)).toThrow('token changed');
        expect(store.recoverMutex(id,lock.token,true)).toBe(true);expect(store.claim('confirm','recovered')).toBeDefined();
        writeFileSync(join(root,id,'.mutex'),JSON.stringify({pid:process.pid,nonce:'live',created:0}));
        expect(()=>store.recoverMutex(id,store.mutexLock(id)!.token,true)).toThrow('is alive');
    } finally {rmSync(root,{recursive:true});}
});

test('checkpoint action equality ignores object key order', async()=>{
    const output=await supervise({command:process.execPath,args:[worker,'reorder'],trace:{...trace,steps:[{op:'tick',payload:{a:1,b:2},by:3}]},timeoutMs:3000});
    expect(output.result.status).toBe('pass');expect(output.journal).toHaveLength(3);
});

test('CLI bounds both domain module initialization and synchronous generation hangs', async()=>{
    const root=temp(), {spawn}=await import('node:child_process');try{
        const cli=join(import.meta.dir,'..','cli.ts');
        for(const mode of ['module','generator']){
            const entry=join(root,`${mode}.ts`),results=join(root,`${mode}-results`);
            writeFileSync(entry,`export const spec={contract:'hang/1',init:()=>0,step:m=>m};export const driver={init:()=>0,step:()=>{}};export function generate(){${mode==='generator'?'while(true){}':"return {format:'roam/1',contract:'hang/1',steps:[]};"}}${mode==='module'?'while(true){}':''}`);
            const began=Date.now();
            const code=await new Promise<number|null>((resolve,reject)=>{
                const child=spawn(process.execPath,[cli,'explore','--driver',entry,'--build','hang-fixture','--root',results,'--timeout','400','--budget','2000','--cases','2'],{windowsHide:true});
                let err='';child.stderr.on('data',x=>err+=x);child.stdout.resume();const timer=setTimeout(()=>{child.kill();reject(new Error('unbounded controller: '+err));},6000);
                child.once('error',reject);child.once('close',code=>{clearTimeout(timer);resolve(code);});
            });
            expect(code).toBe(2);expect(Date.now()-began).toBeLessThan(5000);
            const store=new FileStore(results);expect(store.list()).toHaveLength(1);const report=store.read(store.list()[0]);expect(report.result.status).toBe('error');expect(report.result.issues[0].message).toContain('generator timeout');
        }
    }finally{rmSync(root,{recursive:true});}
},15000);


async function cliProcess(args: string[], timeout=12000) {
    const {spawn}=await import('node:child_process');
    return new Promise<{code:number|null;stdout:string;stderr:string}>((resolve,reject)=>{
        const child=spawn(process.execPath,[join(import.meta.dir,'..','cli.ts'),...args],{windowsHide:true});let stdout='',stderr='';
        const timer=setTimeout(()=>{child.kill();reject(new Error('CLI supervisor test timeout: '+stderr));},timeout);
        child.stdout.on('data',x=>stdout+=x);child.stderr.on('data',x=>stderr+=x);child.once('error',reject);child.once('close',code=>{clearTimeout(timer);resolve({code,stdout,stderr});});
    });
}

test('stored snapshot replays and shrinks after all original driver sources are removed',async()=>{
    const root=temp();try{
        const entry=join(root,'domain.ts'),results=join(root,'evidence');
        const executePath=join(import.meta.dir,'..','core.ts').replaceAll('\\','/');
        writeFileSync(entry,`import {property} from ${JSON.stringify(executePath)};export const spec={contract:'independent/1',init:()=>0,step:m=>m+1,check:(m,o)=>property('value',m===o)};export const driver={init:()=>({v:0}),step:r=>{r.v+=2},observe:r=>r.v};export const generate=()=>({format:'roam/1',contract:'independent/1',steps:[{op:'add'}]});`);
        const first=await cliProcess(['explore','--driver',entry,'--build','original','--root',results,'--cases','1']);expect(first.code).toBe(0);
        const store=new FileStore(results),id=store.list()[0];expect(store.read(id).result.status).toBe('fail');
        rmSync(entry);
        const replay=await cliProcess(['replay','--id',id,'--root',results]);expect(replay.code).toBe(1);expect(JSON.parse(replay.stdout).classification).toBe('stable');
        const reduced=await cliProcess(['reduce','--id',id,'--root',results,'--cases','10']);expect(reduced.code).toBe(0);expect(JSON.parse(reduced.stdout).reduced.result.failure.propertyId).toBe('value');
        expect(store.list()).toHaveLength(1);
        const stored=store.read(id),snapshot=stored.environment.driver;writeFileSync(snapshot,'// corrupted');
        const tampered=await cliProcess(['replay','--id',id,'--root',results]);expect(tampered.code).toBe(2);expect(tampered.stderr).toContain('hash mismatch');
    }finally{rmSync(root,{recursive:true});}
},20000);

test('exploration saves crash and hang paths then continues with a fresh passing worker',async()=>{
    const root=temp();try{
        const entry=join(root,'domain.ts'),results=join(root,'evidence');
        writeFileSync(entry,`export const spec={contract:'continue/1',init:()=>0,step:m=>m};export const driver={init:()=>0,step:async(r,a)=>{if(a.op==='crash')process.exit(19);if(a.op==='hang')await new Promise(()=>{});}};export const generate=seed=>({format:'roam/1',contract:'continue/1',steps:[{op:seed===1?'crash':seed===2?'hang':'ok'}]});`);
        const run=await cliProcess(['explore','--driver',entry,'--build','faults','--root',results,'--cases','3','--timeout','500']);
        const store=new FileStore(results);expect(store.list()).toHaveLength(2);expect(store.list().map(id=>store.read(id).trace.steps[0].op).sort()).toEqual(['crash','hang']);
        const hang=store.list().map(id=>store.read(id)).find(record=>record.trace.steps[0].op==='hang')!;
        if(hang.result.issues.some(issue=>issue.name==='TreeCleanupError')) {
            expect(run.code).toBe(2);expect(run.stderr).toContain('exploration stopped');expect(run.stdout.trim()).toBe('');
            expect(hang.result.issues[0].name).toBe('Timeout');expect(hang.result.reason).toBe('timeout');
        }else{
            expect(run.code).toBe(0);const summary=JSON.parse(run.stdout);expect(summary.completed).toBe(3);expect(summary.counts.error).toBe(2);expect(summary.counts.pass).toBe(1);
        }
        const keys=store.list().map(id=>failureKey(store.read(id).result));expect(new Set(keys).size).toBe(2);
        for(const id of store.list()) {
            expect(readFileSync(join(results,id,'attempt','history.jsonl'),'utf8')).toContain('before');
            const replay=await cliProcess(['replay','--id',id,'--root',results,'--timeout','500']);
            if(store.read(id).result.issues.some(issue=>issue.name==='TreeCleanupError')) {expect(replay.code).toBe(2);expect(replay.stderr).toContain('exploration stopped');}
            else{expect(replay.code).toBe(1);expect(JSON.parse(replay.stdout).classification).toBe('stable');}
        }
        const zero=await cliProcess(['gate','--snapshot',store.read(store.list()[0]).environment.entryHash,'--root',results,'--cases','0']);expect(zero.code).toBe(2);expect(zero.stderr).toContain('positive integer');
    }finally{rmSync(root,{recursive:true});}
},15000);


test('malformed result issues and action frames outside the case never pass',async()=>{
    for(const mode of ['badissues','postterminal','extrabefore','hiddenfailure']){
        const output=await supervise({command:process.execPath,args:[worker,mode],trace:{...trace,steps:[]},timeoutMs:3000});
        expect(output.result.status).toBe('error');expect(output.result.issues[0].name).toBe('ProtocolError');
    }
});


test('failure identity separates exit, timeout and protocol classes without diagnostic messages',async()=>{
    const outputs=[];
    for(const mode of ['crash','hang','malformed'])outputs.push(await supervise({command:process.execPath,args:[worker,mode],trace,timeoutMs:mode==='hang'?500:3000}));
    const keys=outputs.map(x=>failureKey(x.result));expect(new Set(keys).size).toBe(3);
    for(const output of outputs){const changed=structuredClone(output.result);changed.issues[0].message='completely different address and timestamp';expect(failureKey(changed)).toBe(failureKey(output.result));}
});


test('error results cannot underreport completed checkpoint actions',async()=>{
    const output=await supervise({command:process.execPath,args:[worker,'underreport'],trace,timeoutMs:3000});
    expect(output.result.status).toBe('error');expect(output.result.issues[0].name).toBe('ProtocolError');expect(output.result.reason).toContain('step count');
});


test('explicit Node runtime is the direct executor and stored replay reuses its identity',async()=>{
    const root=temp();try{
        const {createHash}=await import('node:crypto');
        const source=`import {createInterface} from 'node:readline';
if(globalThis.Bun)process.exit(31);
const lines=createInterface({input:process.stdin}),generating=process.argv[2]==='generate';
const send=x=>process.stdout.write(JSON.stringify(x)+'\\n');
send({type:'hello',format:generating?'roam-generator/1':'roam-worker/1',capabilities:[]});
lines.on('line',line=>{const m=JSON.parse(line);if(m.type==='generate'){send({type:'case',case:{format:'roam/1',contract:'node/1',steps:[{op:'go'}]}});lines.close();process.stdin.pause();}
else if(m.type==='run')send({type:'before',step:0,action:m.case.steps[0]});
else if(m.type==='ack'){send({type:'after',step:0});send({type:'result',result:{status:'fail',steps:1,issues:[],failure:{propertyId:'node-value',message:'node failure'}}});lines.close();process.stdin.pause();}});`;
        const hash=createHash('sha256').update(source).digest('hex'),directory=join(root,'.snapshots',hash);mkdirSync(directory,{recursive:true});writeFileSync(join(directory,'driver.mjs'),source);
        const first=await cliProcess(['explore','--snapshot',hash,'--runtime','node','--root',root,'--cases','1']);expect(first.code).toBe(0);
        const store=new FileStore(root),id=store.list()[0],info=store.read(id).environment.runtime;expect(info.kind).toBe('node');expect(info.version).toMatch(/^v[0-9]/);expect(info.executable).toContain('node');
        const replay=await cliProcess(['replay','--id',id,'--root',root]);expect(replay.code).toBe(1);expect(JSON.parse(replay.stdout).classification).toBe('stable');
        const invalid=await cliProcess(['gate','--snapshot',hash,'--runtime','python','--root',root]);expect(invalid.code).toBe(2);expect(invalid.stderr).toContain('node or bun');
    }finally{rmSync(root,{recursive:true});}
},15000);


test('Bun generated __require helper cannot bypass dynamic dependency rejection',async()=>{
    const root=temp();try{
        const {freezeDriver}=await import('../snapshot'),entry=join(root,'dynamic-require.ts');
        writeFileSync(entry,`export const spec={};export const driver={};export function generate(){const name=globalThis.runtimeModule;return require(name)}`);
        await expect(freezeDriver(entry,root)).rejects.toThrow('dynamic module loading');
    }finally{rmSync(root,{recursive:true});}
});


test('original snapshot replay rejects runtime version drift unless explicitly selected',async()=>{
    const root=temp();try{
        const first=await cliProcess(['explore','--driver',join(import.meta.dir,'fixtures','counter.ts'),'--build','runtime-pinned','--root',root,'--cases','1','--steps','1','--seed','13']);expect(first.code).toBe(0);
        const store=new FileStore(root),originalId=store.list()[0],original=store.read(originalId);
        // Publish an independent historical environment, never edit the immutable original.
        const historical=store.publish(original.trace,original.result,{...original.environment,runtime:{...original.environment.runtime,version:'different-historical-version'}});
        const replay=await cliProcess(['replay','--id',historical,'--root',root]);expect(replay.code).toBe(2);expect(replay.stderr).toContain('runtime version mismatch');
        const explicit=await cliProcess(['replay','--id',historical,'--runtime','bun','--root',root]);expect(explicit.code).toBe(1);expect(JSON.parse(explicit.stdout).classification).toBe('stable');
        expect(store.read(originalId).environment.runtime.version).toBe(original.environment.runtime.version);
    }finally{rmSync(root,{recursive:true});}
},15000);


test('tree cleanup denial preserves timeout evidence and makes the explorer stop',async()=>{
    const root=temp();try{
        const store=new FileStore(root),output=await supervise({command:process.execPath,args:[worker,'hang'],trace,timeoutMs:500,store});
        expect(output.result.issues[0].name).toBe('Timeout');expect(output.result.reason).toBe('timeout');expect(output.evidenceId).toBeDefined();
        expect(store.read(output.evidenceId!).result).toEqual(output.result);
        const entry=join(root,'domain.ts'),results=join(root,'cli');
        writeFileSync(entry,`export const spec={contract:'cleanup-stop/1',init:()=>0,step:m=>m};export const driver={init:()=>0,step:async()=>{await new Promise(()=>{})}};export const generate=()=>({format:'roam/1',contract:'cleanup-stop/1',steps:[{op:'hang'}]});`);
        const cli=await cliProcess(['explore','--driver',entry,'--build','cleanup-fixture','--root',results,'--cases','2','--timeout','500']);
        const reports=new FileStore(results),all=reports.list().map(id=>reports.read(id));
        if(output.result.issues.some(issue=>issue.name==='TreeCleanupError')) {
            expect(cli.code).toBe(2);expect(cli.stderr).toContain('exploration stopped');expect(all).toHaveLength(1);
            expect(all[0].result.issues[0].name).toBe('Timeout');expect(all[0].result.issues.some(x=>x.phase==='host-cleanup'&&x.name==='TreeCleanupError')).toBe(true);
        }else{expect(cli.code).toBe(0);expect(all).toHaveLength(2);}
    }finally{rmSync(root,{recursive:true});}
},15000);


async function waitFile(path:string,ms=3000){const end=Date.now()+ms;while(!existsSync(path)){if(Date.now()>end)throw new Error('fixture did not create '+path);await new Promise(r=>setTimeout(r,20));}}
function pidAlive(pid:number){try{process.kill(pid,0);return true;}catch(error:any){if(error.code==='ESRCH')return false;throw error;}}

test('hard-stop owns derived heartbeat processes or reports cleanup failure without pretending success',async()=>{
    const root=temp();try{
        for(const mode of ['worker','generate']){
            const directory=join(root,mode);mkdirSync(directory);let pid:number|undefined;
            try{
                const args=[join(import.meta.dir,'fixtures','tree-worker.ts'),mode,directory];
                let denied=false,cleanupMessage='';
                if(mode==='worker'){
                    const store=new FileStore(join(directory,'evidence'));
                    const result=await supervise({command:process.execPath,args,trace,timeoutMs:1500,store});
                    expect(result.result.reason).toBe('timeout');expect(result.result.issues[0].name).toBe('Timeout');
                    expect(store.read(result.evidenceId!).trace).toEqual(trace);
                    const cleanup=result.result.issues.find(issue=>issue.phase==='host-cleanup'&&issue.name==='TreeCleanupError');denied=!!cleanup;cleanupMessage=cleanup?.message??'';
                }else{
                    let failure:any;try{await generateCase({command:process.execPath,args,seed:1,steps:1,timeoutMs:1500});}catch(error){failure=error;}
                    expect(failure?.message).toContain('generator timeout');denied=!!failure?.cleanupError;cleanupMessage=failure?.cleanupError?.message??'';
                }
                await waitFile(join(directory,'pid'));pid=Number(readFileSync(join(directory,'pid'),'utf8'));
                if(denied){
                    // A denied tree kill must be reported, never claimed successful. The
                    // test itself cooperatively stops its known descendant in finally.
                    expect(cleanupMessage).toContain('cleanup failed');
                    if(pidAlive(pid)){
                        const prior=readFileSync(join(directory,'heartbeat'),'utf8');await new Promise(r=>setTimeout(r,100));
                        expect(readFileSync(join(directory,'heartbeat'),'utf8').length).toBeGreaterThan(prior.length);
                    }
                    // The OS may also close a descendant when its root dies. That does
                    // not turn the denied tree operation into a claimed success.
                }else{
                    const until=Date.now()+1500;while(pidAlive(pid)&&Date.now()<until)await new Promise(r=>setTimeout(r,20));
                    expect(pidAlive(pid)).toBe(false);
                    const stopped=readFileSync(join(directory,'heartbeat'),'utf8');await new Promise(r=>setTimeout(r,100));expect(readFileSync(join(directory,'heartbeat'),'utf8')).toBe(stopped);
                }
            }finally{
                writeFileSync(join(directory,'stop'),'stop');
                if(pid===undefined&&existsSync(join(directory,'pid')))pid=Number(readFileSync(join(directory,'pid'),'utf8'));
                if(pid!==undefined){const end=Date.now()+2000;while(pidAlive(pid)&&Date.now()<end)await new Promise(r=>setTimeout(r,20));expect(pidAlive(pid)).toBe(false);}
            }
        }
    }finally{rmSync(root,{recursive:true});}
},15000);


test('slow failed tree cleanup cannot replace an earlier protocol failure with timeout',async()=>{
    const {spawn}=await import('node:child_process');
    const report=await new Promise<{result:RunResult;elapsed:number}>((resolve,reject)=>{
        const child=spawn(process.execPath,[join(import.meta.dir,'fixtures','primary-error.ts')],{windowsHide:true});let stdout='',stderr='';
        const timer=setTimeout(()=>{child.kill();reject(new Error('fault-injection fixture timeout: '+stderr));},6000);
        child.stdout.on('data',x=>stdout+=x);child.stderr.on('data',x=>stderr+=x);child.once('error',reject);child.once('close',code=>{clearTimeout(timer);if(code!==0)return reject(new Error(stderr));try{resolve(JSON.parse(stdout));}catch(error){reject(error);}});
    });
    expect(report.elapsed).toBeGreaterThanOrEqual(1000);expect(report.result.issues[0].name).toBe('ProtocolError');expect(report.result.reason).toBe('protocol:json');
    expect(report.result.issues.some(issue=>issue.phase==='host-cleanup'&&issue.name==='TreeCleanupError')).toBe(true);
},8000);
