/** Local immutable evidence and leases. Publication fsyncs individual files before same-volume rename;
 * this protects process interruption, not a filesystem/power-loss transaction. Original paths
 * are never overwritten through this API. Recover stale mutexes/pending folders only after
 * independently establishing their producer is dead.
 */
import { mkdirSync, writeFileSync, readFileSync, renameSync, readdirSync, openSync, closeSync, fsyncSync, unlinkSync, existsSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { encodeCase, decodeCase, assertJson, type RoamCase, type RunResult, type Json } from './core';
export function durable(path: string, value: unknown) {
    const fd = openSync(path, 'wx');
    try { writeFileSync(fd, JSON.stringify(value)); fsyncSync(fd); } finally { closeSync(fd); }
}
function atomic(path: string, value: unknown) {
    const tmp = `${path}.${randomUUID()}.tmp`;
    try { durable(tmp, value); renameSync(tmp, path); } finally { if (existsSync(tmp)) unlinkSync(tmp); }
}
function lockInfo(path: string) {
    if (!existsSync(path)) return;
    const raw = readFileSync(path), token = createHash('sha256').update(raw).digest('hex');
    let owner: { pid: number; nonce: string; created: number } | null = null;
    try { const value = JSON.parse(raw.toString()); if (Number.isSafeInteger(value.pid) && value.pid > 0 && typeof value.nonce === 'string' && Number.isFinite(value.created)) owner = value; } catch {}
    return { token, owner };
}
function recoverOwnedLock(path: string, token: string, allWritersStopped: boolean) {
    if (!allWritersStopped) throw new Error('lock recovery requires all writers stopped');
    const lock = lockInfo(path); if (!lock) return false;
    if (lock.token !== token) throw new Error('lock token changed');
    if (lock.owner) {
        let alive = true;
        try { process.kill(lock.owner.pid, 0); } catch (error: any) { if (error.code === 'ESRCH') alive = false; else throw new Error('cannot establish lock owner exit', { cause: error }); }
        if (alive) throw new Error(`lock owner ${lock.owner.pid} is alive`);
    }
    renameSync(path, `${path}.recovered-${randomUUID()}.json`); return true;
}
export function validateResult(value: unknown, maxSteps: number): asserts value is RunResult {
    assertJson(value);
    const r = value as unknown as RunResult;
    if (!r || typeof r !== 'object' || !['pass', 'fail', 'invalid', 'error', 'unsupported', 'incomplete'].includes(r.status) || !Number.isSafeInteger(r.steps) || r.steps < 0 || r.steps > maxSteps || !Array.isArray(r.issues)) throw new Error('invalid worker result');
    if (r.issues.some(x => !x || typeof x !== 'object' || typeof x.phase !== 'string' || typeof x.name !== 'string' || typeof x.message !== 'string' || (x.step !== undefined && (!Number.isSafeInteger(x.step) || x.step < -1 || x.step > maxSteps)))) throw new Error('invalid worker issues');
    if (r.failure !== undefined && (!r.failure || typeof r.failure.propertyId !== 'string' || !r.failure.propertyId || typeof r.failure.message !== 'string')) throw new Error('invalid worker failure');
    if (r.failure && r.status !== 'fail' && r.status !== 'error') throw new Error('failure incompatible with result status');
    if (r.status === 'fail' && !r.failure) throw new Error('failure missing property identity');
    if (r.reason !== undefined && typeof r.reason !== 'string') throw new Error('invalid worker reason');
    if (r.history !== undefined && (!Array.isArray(r.history) || r.history.some(x => !x || typeof x !== 'object' || typeof x.phase !== 'string'))) throw new Error('invalid worker history');
}
export type Lease = { id: string; token: string; role: string; worker: string; until: number };
export class FileStore {
    readonly root: string;
    constructor(root: string, readonly maxBytes = Infinity) { this.root = resolve(root); mkdirSync(this.root, { recursive: true }); }
    list() { return readdirSync(this.root, { withFileTypes: true }).filter(x => x.isDirectory() && !x.name.startsWith('.')).map(x => x.name).filter(id => existsSync(join(this.root, id, 'ready.json'))); }
    read(id: string) { const dir = this.dir(id); return { trace: decodeCase(readFileSync(join(dir, 'case.json'), 'utf8')), result: JSON.parse(readFileSync(join(dir, 'failure.json'), 'utf8')) as RunResult, environment: JSON.parse(readFileSync(join(dir, 'environment.json'), 'utf8')) }; }
    private dir(id: string) { if (!/^[a-zA-Z0-9_-]+$/.test(id)) throw new Error('invalid evidence id'); return join(this.root, id); }
    publish(trace: RoamCase, result: RunResult, environment: Json = null) {
        const encoded = encodeCase(trace); validateResult(result, trace.steps.length); assertJson(environment);
        const capacityLock = Number.isFinite(this.maxBytes) ? join(this.root, '.capacity.lock') : undefined;
        const capacityFd = capacityLock ? openSync(capacityLock, 'wx') : undefined;
        try {
        if (capacityFd !== undefined) { writeFileSync(capacityFd, JSON.stringify({ pid: process.pid, nonce: randomUUID(), created: Date.now() })); fsyncSync(capacityFd); }
        const bytes = Buffer.byteLength(encoded) + Buffer.byteLength(JSON.stringify(result)) + Buffer.byteLength(JSON.stringify(environment));
        if (this.bytes() + bytes > this.maxBytes) throw new Error('evidence capacity exhausted');
        const id = randomUUID(), pending = join(this.root, `.pending-${id}`), target = this.dir(id);
        mkdirSync(pending);
        try {
            durable(join(pending, 'case.json'), JSON.parse(encoded)); durable(join(pending, 'failure.json'), result); durable(join(pending, 'environment.json'), environment);
            const env = environment && typeof environment === 'object' && !Array.isArray(environment) ? environment : {};
            const cluster = createHash('sha256').update(JSON.stringify([trace.contract, failureKey(result), env.entryHash ?? null, env.build ?? trace.meta?.build ?? null])).digest('hex');
            durable(join(pending, 'ready.json'), { id, cluster, created: Date.now() });
            renameSync(pending, target);
            return id;
        } catch (error) { throw new Error(`evidence publication failed; pending retained at ${pending}`, { cause: error }); }
        } finally { if (capacityFd !== undefined) { closeSync(capacityFd); unlinkSync(capacityLock!); } }
    }
    /** Read-only identity used by the explicit, offline recovery command. */
    capacityLock() { return lockInfo(join(this.root, '.capacity.lock')); }
    /** Offline maintenance: stop all writers, inspect current token, never remove a live owner. */
    recoverCapacityLock(token: string, allWritersStopped: boolean) { return recoverOwnedLock(join(this.root, '.capacity.lock'), token, allWritersStopped); }
    bytes() { const size = (dir: string): number => readdirSync(dir, { withFileTypes: true }).reduce((sum, e) => sum + (e.isDirectory() ? size(join(dir, e.name)) : statSync(join(dir, e.name)).size), 0); return size(this.root); }
    pending() { return readdirSync(this.root).filter(x => x.startsWith('.pending-') || x.startsWith('.run-')); }
    /** Explicit operator recovery only: caller must establish the producing worker is dead. */
    recoverPending(name: string) {
        if (!/^\.(pending|run)-[a-zA-Z0-9_-]+$/.test(name)) throw new Error('invalid pending name');
        const dir = join(this.root, name), trace = decodeCase(readFileSync(join(dir, 'case.json'), 'utf8'));
        const id = this.publish(trace, {status: 'error', steps: 0, issues: [{phase: 'host', name: 'InterruptedPublication', message: 'producer interrupted; original evidence retained'}]}, { recoveredFrom: name });
        renameSync(dir, join(this.root, id, 'recovered')); return id;
    }
    private locked<T>(id: string, work: (dir: string) => T): T {
        const dir = this.dir(id), lock = join(dir, '.mutex');
        // Never steal a live critical-section lock. A crashed operation is visible and can be recovered explicitly.
        const fd = openSync(lock, 'wx');
        try { writeFileSync(fd, JSON.stringify({ pid: process.pid, nonce: randomUUID(), created: Date.now() })); fsyncSync(fd); return work(dir); } finally { closeSync(fd); unlinkSync(lock); }
    }
    mutexLock(id: string) { return lockInfo(join(this.dir(id), '.mutex')); }
    /** Same offline owner/token discipline as capacity recovery; mtime never authorizes stealing. */
    recoverMutex(id: string, token: string, allWritersStopped: boolean) { return recoverOwnedLock(join(this.dir(id), '.mutex'), token, allWritersStopped); }
    claim(role: string, worker: string, leaseMs = 60000): Lease | undefined {
        if (!(leaseMs > 0)) throw new Error('lease must be positive');
        for (const id of this.list()) {
            try { const lease = this.locked(id, dir => {
                const done = join(dir, `${this.role(role)}.done.json`), path = join(dir, `${this.role(role)}.lease.json`);
                if (existsSync(done)) return;
                if (existsSync(path) && JSON.parse(readFileSync(path, 'utf8')).until > Date.now()) return;
                const value = { id, token: randomUUID(), role, worker, until: Date.now() + leaseMs }; atomic(path, value); return value;
            }); if (lease) return lease; } catch (e: any) { if (e.code !== 'EEXIST') throw e; }
        }
    }
    private role(role: string) { if (!/^[a-zA-Z0-9_-]+$/.test(role)) throw new Error('invalid queue role'); return role; }
    renew(lease: Lease, leaseMs = 60000) { return this.locked(lease.id, dir => { const path = join(dir, `${this.role(lease.role)}.lease.json`), current = JSON.parse(readFileSync(path, 'utf8')); if (current.token !== lease.token || current.until <= Date.now()) throw new Error('lease lost'); const next = { ...lease, until: Date.now() + leaseMs }; atomic(path, next); return next; }); }
    complete(lease: Lease, outcome: Json) { assertJson(outcome); this.locked(lease.id, dir => { const path = join(dir, `${this.role(lease.role)}.lease.json`), current = JSON.parse(readFileSync(path, 'utf8')); if (current.token !== lease.token || current.until <= Date.now()) throw new Error('lease lost'); durable(join(dir, `${this.role(lease.role)}.done.json`), { token: lease.token, outcome }); unlinkSync(path); }); }
    reduced(id: string, trace: RoamCase, result: RunResult) { const dir = this.dir(id); durable(join(dir, `reduced-${randomUUID()}.json`), { case: JSON.parse(encodeCase(trace)), result }); }
    statistics(value: Json, worker = `pid-${process.pid}`) { assertJson(value); if (!/^[a-zA-Z0-9_-]+$/.test(worker)) throw new Error('invalid worker id'); atomic(join(this.root, `statistics-${worker}.json`), value); }
}
/** Stable reproduction identity; diagnostic messages and addresses never define a failure. */
export function failureKey(result: RunResult): string | undefined {
    if (result.status === 'pass') return;
    return JSON.stringify(result.status === 'fail' ? ['fail', result.failure?.propertyId] : [result.status, result.reason ?? null, result.issues.map(issue => [issue.phase, issue.name])]);
}
