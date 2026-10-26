import {test, expect} from 'bun:test';
import {execute, PropertyFailure, type RoamSpec, type RoamDriver, type RoamCase} from '../core';
const c: RoamCase = {format: 'roam/1', contract: 'counter/1', steps: [{op: 'inc'}, {op: 'inc'}]};
const spec: RoamSpec<number> = {contract: c.contract, init: () => 0, step: m => m + 1, check(m, v) {if (m !== v) throw new PropertyFailure('counter-value', 'mismatch', m, v);}};
const driver = (): RoamDriver<{n: number}> => ({init: () => ({n: 0}), step(r) {r.n++;}, observe: r => r.n});
test('roam-execute-01 strict replay and named counterexample', async () => {
    expect((await execute(spec, driver(), c, {history: true})).status).toBe('pass');
    const bad = driver(); bad.step = r => {r.n += 2;};
    const result = await execute(spec, bad, c, {history: true});
    expect(result.status).toBe('fail'); expect(result.failure?.propertyId).toBe('counter-value'); expect(result.steps).toBe(1);
    expect(result.history?.some(x => x.phase === 'call')).toBe(true);
});
test('roam-execute-02 partial init and primary/cleanup errors remain observable', async () => {
    const cleanup: string[] = [];
    const d = driver();
    d.init = (_, ctx) => {ctx.defer(() => {cleanup.push('partial'); throw Error('partial cleanup');}); throw new PropertyFailure('init-property', 'bad init');};
    d.dispose = r => {expect(r).toBeUndefined(); cleanup.push('dispose'); throw Error('dispose failed');};
    const r = await execute(spec, d, c);
    expect(r.status).toBe('error'); expect(r.failure?.propertyId).toBe('init-property'); expect(r.issues).toHaveLength(3);
    expect(cleanup).toEqual(['dispose', 'partial']);
});
test('roam-execute-03 dependency and precondition failures are invalid', async () => {
    const s: RoamSpec<number> = {...spec, dependencies: () => ({requires: ['missing']})};
    expect((await execute(s, driver(), c)).status).toBe('invalid');
    expect((await execute({...spec, valid: () => false}, driver(), c)).status).toBe('invalid');
});
test('roam-execute-04 unsupported, abort and budget never pass', async () => {
    expect((await execute(spec, driver(), {...c, requires: ['network']})).status).toBe('unsupported');
    expect((await execute(spec, driver(), c, {signal: {aborted: true}})).status).toBe('incomplete');
    const r = await execute(spec, driver(), c, {maxSteps: 1}); expect(r.status).toBe('incomplete'); expect(r.steps).toBe(1);
});
test('roam-execute-05 final checks and before-step host acknowledgement', async () => {
    const order: string[] = [], d = driver(); d.step = r => {order.push('step'); r.n++;};
    const r = await execute({...spec, done() {throw new PropertyFailure('final', 'final invariant');}}, d, c, {beforeStep: i => {order.push(`ack${i}`);}});
    expect(r.failure?.propertyId).toBe('final'); expect(order).toEqual(['ack0', 'step', 'ack1', 'step']);
});
test('roam-execute-06 cleanup consumes the same total budget', async () => {
    let time = 0;
    const d = driver();
    d.init = (_, ctx) => {ctx.defer(() => {time += 3;}); return {n: 0};};
    d.dispose = () => {time += 3;};
    const r = await execute(spec, d, c, {budgetMs: 5, now: () => time});
    expect(r.status).toBe('incomplete'); expect(r.reason).toBe('time-budget'); expect(r.steps).toBe(2);
});
test('roam-execute-07 schedule cannot be silently ignored by a driver', async () => {
    let initialized = false;
    const d = driver(); d.capabilities = ['schedule']; d.init = () => {initialized = true; return {n: 0};};
    const r = await execute(spec, d, {...c, schedule: [{deliver: 'task-1'}]}, {history: true});
    expect(r.status).toBe('unsupported'); expect(initialized).toBe(false); expect(r.steps).toBe(0);
    expect((await execute(spec, driver(), {...c, schedule: []})).status).toBe('pass');
});
test('roam-execute-08 async preconditions and host ack cannot dispatch after abort or budget', async () => {
    let calls = 0, time = 0;
    const d = driver(); d.step = () => {calls++;};
    const r = await execute({...spec, valid: async () => {time = 10; return true;}}, d, c, {now: () => time, budgetMs: 5});
    expect(r.status).toBe('incomplete'); expect(calls).toBe(0);
    const signal = {aborted: false};
    const aborted = await execute(spec, d, c, {signal, beforeStep: async () => {signal.aborted = true;}});
    expect(aborted.status).toBe('incomplete'); expect(calls).toBe(0);
});
test('roam-execute-09 failed initialization budget prevents initializing real resources', async () => {
    let time = 0, initialized = 0;
    const d = driver(); d.init = () => {initialized++; return {n: 0};};
    const r = await execute({...spec, init: () => {time = 10; return 0;}}, d, c, {now: () => time, budgetMs: 5});
    expect(r.status).toBe('incomplete'); expect(initialized).toBe(0);
});
test('roam-execute-10 failure payload is snapshotted and invalid JSON becomes infrastructure error', async () => {
    const expected = {value: 1}, d = driver(); d.dispose = () => {expected.value = 2;};
    const r = await execute({...spec, check() {throw new PropertyFailure('snapshot', 'bad', expected);}}, d, c);
    expect(r.failure?.expected).toEqual({value: 1});
    const invalid = await execute({...spec, check() {throw new PropertyFailure('nan', 'bad', NaN);}}, driver(), c);
    expect(invalid.status).toBe('error'); expect(invalid.issues.some(i => i.phase === 'failure-encoding')).toBe(true);
});
test('roam-execute-11 completed driver actions remain counted when model transition fails', async () => {
    const r = await execute({...spec, step() {throw new PropertyFailure('transition', 'model violation');}}, driver(), c, {history: true});
    expect(r.status).toBe('fail'); expect(r.failure?.propertyId).toBe('transition'); expect(r.steps).toBe(1);
    expect(r.history?.filter(h => h.phase === 'complete')).toHaveLength(1);
});
