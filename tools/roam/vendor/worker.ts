/** Supervised JSONL workers; each call owns its process tree and deadline. */
import {mkdirSync, openSync, writeFileSync, fsyncSync, closeSync, rmSync, renameSync} from 'node:fs';
import {join} from 'node:path';
import {randomUUID} from 'node:crypto';
import {spawn, execFile, type ChildProcess} from 'node:child_process';
import {createInterface} from 'node:readline';
import { encodeCase, decodeCase, assertJson, type RoamCase, type RunResult, type Json } from './core';
import {FileStore, durable, validateResult} from './store';
function jsonEqual(a: Json, b: Json): boolean {
    if (a === b) return true;
    if (!a || !b || typeof a !== 'object' || typeof b !== 'object' || Array.isArray(a) !== Array.isArray(b)) return false;
    if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((value, index) => jsonEqual(value, b[index]));
    const ak = Object.keys(a).sort(), bk = Object.keys(b).sort();
    return ak.length === bk.length && ak.every((key, index) => key === bk[index] && jsonEqual((a as { [key: string]: Json })[key], (b as { [key: string]: Json })[key]));
}
/** Kill only a tree rooted in a process spawned by this host; no global PID discovery. */
async function terminateOwnedTree(child: ChildProcess): Promise<void> {
    if (!child.pid) return;
    if (process.platform !== 'win32') {
        try { process.kill(-child.pid, 'SIGKILL'); } catch (error: any) { if (error.code !== 'ESRCH') throw error; }
        return;
    }
    // Hard-stop only. Normal workers own descendant cleanup before natural exit;
    // detached/orphaned trees are outside this deliberately small host contract.
    if (child.exitCode !== null || child.signalCode !== null) throw new Error('owned process tree cleanup failed: worker exited before tree verification');
    await new Promise<void>((resolve, reject) => {
        execFile('taskkill', ['/F', '/T', '/PID', String(child.pid)], { timeout: 5000, windowsHide: true }, (error, _out, stderr) => {
            if (!error) resolve();
            else reject(new Error(`owned process tree cleanup failed: ${stderr || error.message}`));
        });
    });
}
function treeStopper(child: ChildProcess) {
    let stopped: Promise<void> | undefined;
    return () => stopped ??= terminateOwnedTree(child);
}
export type WorkerHello = { type: 'hello'; format: 'roam-worker/1'; capabilities: string[] };
export type SuperviseOptions = { command: string; args?: string[]; trace: RoamCase; timeoutMs: number; cwd?: string; required?: string[]; store?: FileStore; environment?: Json; maxOutputBytes?: number };
export type Supervised = { result: RunResult; evidenceId?: string; stderr: string; journal: string[] };
function issue(name: string, message: string, steps: number, status: RunResult['status'] = 'error'): RunResult { return { status, steps, issues: [{ phase: 'host', name, message }] }; }
/** Owns the worker process tree. Workers must not daemonize/detach descendants or self-exit before cleaning them. */
export async function supervise(options: SuperviseOptions): Promise<Supervised> {
    if (!(options.timeoutMs > 0 && Number.isFinite(options.timeoutMs))) throw new Error('finite positive timeout required');
    encodeCase(options.trace);
    const journal: string[] = [], limit = options.maxOutputBytes ?? 1048576;
    let controlledStop = false;
    let stderr = '', count = 0, steps = 0, completedSteps = 0, activeStep = -1, hello = false, result: RunResult | undefined, failure: Error | undefined;
    const pending = options.store ? join(options.store.root, `.run-${randomUUID()}`) : undefined;
    if (pending) { mkdirSync(pending); durable(join(pending, 'case.json'), JSON.parse(encodeCase(options.trace))); durable(join(pending, 'environment.json'), options.environment ?? null); }
    const record = (value: unknown) => { const line = JSON.stringify(value); journal.push(line); if (pending) { const fd = openSync(join(pending, 'history.jsonl'), 'a'); try { writeFileSync(fd, line + '\n'); fsyncSync(fd); } finally { closeSync(fd); } } };
    const child = spawn(options.command, options.args ?? [], { cwd: options.cwd, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true, detached: process.platform !== 'win32' });
    const stopTree = treeStopper(child);
    let stopping: Promise<void> | undefined, cleanupError: Error | undefined;
    const stop = () => { stopping ??= stopTree().catch(error => { cleanupError = error; child.kill('SIGKILL'); child.stdin.destroy(); child.stdout.destroy(); child.stderr.destroy(); }); };
    const send = (value: unknown) => { if (!child.stdin.destroyed) child.stdin.write(JSON.stringify(value) + '\n'); };
    child.stdin.on('error', () => {});
    const done = new Promise<void>(resolveDone => { child.once('error', e => { failure ??= Object.assign(e, { roamReason: `spawn:${(e as NodeJS.ErrnoException).code ?? e.name}` }); }); child.once('close', (code, signal) => { if ((code !== 0 || signal) && !failure && !controlledStop) failure ??= Object.assign(new Error(`worker exited ${code ?? signal}`), { roamReason: `exit:${code ?? signal}` }); if (!result && !failure) failure ??= Object.assign(new Error(`worker exited ${code ?? signal} without result`), { roamReason: 'missing-result' }); resolveDone(); }); });
    child.stderr.on('data', data => { count += data.length; stderr += data.toString().slice(0, Math.max(0, limit - stderr.length)); if (count > limit) { failure ??= Object.assign(new Error('worker output budget exceeded'), { roamReason: 'output-budget' }); stop(); } });
    child.stdout.on('data', data => { count += data.length; if (count > limit) { failure ??= Object.assign(new Error('worker output budget exceeded'), { roamReason: 'output-budget' }); stop(); } });
    const lines = createInterface({ input: child.stdout });
    lines.on('line', line => { try {
        if (count > limit) throw new Error('worker output budget exceeded');
        const raw = JSON.parse(line); assertJson(raw);
        if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) throw new Error('worker message must be an object');
        const message = raw as Record<string, any>;
        if (!hello) {
            if (message.type !== 'hello' || message.format !== 'roam-worker/1' || !Array.isArray(message.capabilities)) throw new Error('invalid worker handshake');
            hello = true;
            if ((options.required ?? []).some(c => !message.capabilities.includes(c))) { result = issue('Unsupported', 'worker capability missing', 0, 'unsupported'); controlledStop = true; stop(); return; }
            send({ type: 'run', case: options.trace }); return;
        }
        if (result) throw new Error('message after terminal result');
        if (message.type === 'before') {
            if (steps >= options.trace.steps.length || activeStep !== -1 || message.step !== steps || !jsonEqual(message.action, options.trace.steps[steps])) throw new Error('unexpected action checkpoint');
            record(message); activeStep = steps; send({ type: 'ack', step: steps }); steps++;
        } else if (message.type === 'after') {
            if (message.step !== activeStep || activeStep === -1) throw new Error('unexpected action completion');
            record(message); completedSteps++; activeStep = -1;
        } else if (message.type === 'result') {
            validateResult(message.result, completedSteps);
            if (message.result.steps !== completedSteps) throw new Error('result step count differs from completed actions');
            if (message.result.status === 'pass' && (completedSteps !== options.trace.steps.length || message.result.steps !== completedSteps)) throw new Error('pass without complete action checkpoints');
            result = message.result; record(message); child.stdin.end();
        } else throw new Error('unexpected worker message');
    } catch (e) {
        failure ??= Object.assign(new Error(`worker stdout protocol error: ${e instanceof Error ? e.message : String(e)}`), { name: 'ProtocolError', roamReason: e instanceof SyntaxError ? 'protocol:json' : e instanceof TypeError ? 'protocol:json-value' : `protocol:${e instanceof Error ? e.message : 'frame'}` });
        try { record({ type: 'protocol-error', line: line.slice(0, 4096), message: failure.message }); } catch (diskError) { failure ??= diskError as Error; }
        stop();
    } });
    const timer = setTimeout(() => { failure ??= Object.assign(new Error('worker timeout'), { roamReason: 'timeout' }); stop(); }, options.timeoutMs);
    try { await done; } finally { clearTimeout(timer); lines.close(); await stopping; }
    if (failure) result = issue(failure.message === 'worker timeout' ? 'Timeout' : failure.name === 'ProtocolError' ? 'ProtocolError' : 'WorkerError', failure.message, completedSteps);
    if (failure) result!.reason = (failure as Error & { roamReason?: string }).roamReason ?? 'worker';
    result ??= issue('WorkerError', 'worker missing result', steps);
    if (cleanupError) { result.issues.push({phase:'host-cleanup',name:'TreeCleanupError',message:cleanupError.message}); if (result.status === 'pass') result.status = 'error'; }
    if (pending) durable(join(pending, 'stderr.json'), stderr);
    let evidenceId: string | undefined;
    if (result.status !== 'pass' && options.store) evidenceId = options.store.publish(options.trace, result, options.environment ?? null);
    if (pending) { if (evidenceId) renameSync(pending, join(options.store!.root, evidenceId, 'attempt')); else rmSync(pending, { recursive: true }); }
    return { result, evidenceId, stderr, journal };
}
/** Worker adapter: before() waits until the controller durably journals each action. */
export async function serveWorker(run: (trace: RoamCase, before: (step: number) => Promise<void>, after: (step: number) => void) => Promise<RunResult>, capabilities: string[] = []) {
    const lines = createInterface({ input: process.stdin }); let resolveAck: (() => void) | undefined, expected = -1, started = false;
    const send = (value: unknown) => process.stdout.write(JSON.stringify(value) + '\n');
    send({ type: 'hello', format: 'roam-worker/1', capabilities });
    lines.on('line', async line => { try {
        const message = JSON.parse(line);
        if (message.type === 'ack' && message.step === expected && resolveAck) { const done = resolveAck; resolveAck = undefined; done(); return; }
        if (message.type !== 'run' || started) throw new Error('invalid controller message');
        started = true; const trace = decodeCase(JSON.stringify(message.case));
        const before = (step: number) => new Promise<void>(resolve => { if (resolveAck) throw new Error('concurrent action checkpoint'); expected = step; resolveAck = resolve; send({ type: 'before', step, action: trace.steps[step] }); });
        send({ type: 'result', result: await run(trace, before, step => { send({ type: 'after', step }); }) }); lines.close(); process.stdin.pause();
    } catch (e) { send({ type: 'result', result: issue('WorkerError', String(e), 0) }); lines.close(); process.stdin.pause(); } });
}


export type GenerateOptions = { command: string; args?: string[]; seed: number; steps: number; timeoutMs: number; cwd?: string; maxOutputBytes?: number };
/** Generate and import domain code only in an isolated, time-bounded child. */
export async function generateCase(options: GenerateOptions): Promise<RoamCase> {
    if (!(options.timeoutMs > 0 && Number.isFinite(options.timeoutMs))) throw new Error('finite generation timeout required');
    const child = spawn(options.command, options.args ?? [], { cwd: options.cwd, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true, detached: process.platform !== 'win32' });
    const limit = options.maxOutputBytes ?? 1048576;
    const stopTree = treeStopper(child);
    let stopping: Promise<void> | undefined, cleanupError: Error | undefined;
    const stop = () => { stopping ??= stopTree().catch(error => { cleanupError = error; child.kill('SIGKILL'); child.stdin.destroy(); child.stdout.destroy(); child.stderr.destroy(); }); };
    let size = 0, hello = false, value: RoamCase | undefined, failure: Error | undefined, stderr = '';
    child.stdin.on('error', () => {});
    const done = new Promise<void>(resolve => { child.once('error', error => { failure ??= error; }); child.once('close', (code, signal) => { if ((code !== 0 || signal) && !failure) failure ??= new Error(`generator exited ${code ?? signal}`); resolve(); }); });
    child.stderr.on('data', data => { size += data.length; stderr += data.toString().slice(0, Math.max(0, limit - stderr.length)); if (size > limit) { failure ??= new Error('generator output budget exceeded'); stop(); } });
    child.stdout.on('data', data => { size += data.length; if (size > limit) { failure ??= new Error('generator output budget exceeded'); stop(); } });
    const lines = createInterface({ input: child.stdout });
    lines.on('line', line => { try {
        if (size > limit) throw new Error('generator output budget exceeded');
        const message = JSON.parse(line);
        if (!hello) { if (message.type !== 'hello' || message.format !== 'roam-generator/1') throw new Error('invalid generator handshake'); hello = true; child.stdin.write(JSON.stringify({ type: 'generate', seed: options.seed, steps: options.steps }) + '\n'); return; }
        if (message.type === 'error') throw new Error(`generator: ${message.message}`);
        if (message.type !== 'case' || value) throw new Error('unexpected generator response');
        value = decodeCase(JSON.stringify(message.case)); child.stdin.end();
    } catch (error) { failure ??= new Error(`generator protocol error: ${error instanceof Error ? error.message : String(error)}`); stop(); } });
    const timeout = setTimeout(() => { failure ??= new Error('generator timeout'); stop(); }, options.timeoutMs);
    try { await done; } finally { clearTimeout(timeout); lines.close(); await stopping; }
    if (failure || cleanupError) throw Object.assign(new Error(`${failure?.message ?? 'generator cleanup failed'}${stderr ? `; stderr: ${stderr.slice(0, 4096)}` : ''}`), cleanupError ? {cleanupError} : {});
    if (!value) throw new Error('generator exited without a case'); return value;
}
export async function serveGenerator(generate: (seed: number, steps: number) => RoamCase | Promise<RoamCase>) {
    const lines = createInterface({ input: process.stdin }); let started = false;
    const send = (value: unknown) => process.stdout.write(JSON.stringify(value) + '\n');
    send({ type: 'hello', format: 'roam-generator/1' });
    lines.on('line', async line => { try {
        const message = JSON.parse(line);
        if (started || message.type !== 'generate' || !Number.isSafeInteger(message.seed) || !Number.isSafeInteger(message.steps) || message.steps < 0) throw new Error('invalid generation request');
        started = true; const trace = await generate(message.seed, message.steps); encodeCase(trace); send({ type: 'case', case: trace });
    } catch (error) { send({ type: 'error', message: error instanceof Error ? error.message : String(error) }); }
    finally { lines.close(); process.stdin.pause(); } });
}
