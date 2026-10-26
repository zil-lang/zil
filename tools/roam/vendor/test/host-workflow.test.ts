import { test, expect } from 'bun:test';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Roam } from '../roam';

function fixture() {
    const root = mkdtempSync(join(tmpdir(), 'roam-roam-'));
    const roam = new Roam({root, target: {driver: join(import.meta.dir, 'fixtures', 'counter.ts'), build: 'roam-test'}});
    return {root, roam, dispose: () => rmSync(root, {recursive: true, force: true})};
}
test('Roam keeps gate and exploration counters local to each call', async () => {
    const {roam, dispose} = fixture();
    try {
        const first = await roam.gate({cases: 1, steps: 2});
        expect(first).toMatchObject({mode: 'gate', completed: 1, actions: 2, counts: {pass: 1}});
        const second = await roam.explore({cases: 2, steps: 3, seed: 13});
        expect(second).toMatchObject({mode: 'explore', completed: 2, actions: 6, counts: {fail: 1, pass: 1}});
        const third = await roam.gate({cases: 2, steps: 1, seed: 13});
        expect(third).toMatchObject({completed: 1, counts: {fail: 1}, stop: 'gate-failure'});
    } finally {dispose();}
}, 15000);
test('Roam independently claims, replays and reduces immutable stored failures', async () => {
    const {root, roam, dispose} = fixture();
    try {
        await roam.explore({cases: 1, steps: 4, seed: 13});
        const id = roam.store.list()[0], original = JSON.stringify(roam.store.read(id));
        const independent = new Roam({root});
        const replay = await independent.replay({repeat: 2});
        expect(replay).toMatchObject({id, classification: 'stable', reproduced: 2, attempts: 2});
        expect((await independent.replay())).toEqual({mode: 'replay', claimed: false});
        const reduced = await independent.reduce({maxRuns: 20});
        if (!('reduced' in reduced)) throw new Error('expected reduced report');
        expect(reduced.reduced.result.failure?.propertyId).toBe('counter-value');
        expect(reduced.reduced.reduced.steps).toEqual([{op: 'add', by: 13}]);
        expect(JSON.stringify(roam.store.read(id))).toBe(original);
    } finally {dispose();}
}, 15000);
test('Roam rejects unbounded or empty execution configurations', async () => {
    const {roam, dispose} = fixture();
    try {
        await expect(roam.gate({cases: 0})).rejects.toThrow('positive integer');
        await expect(roam.explore({budgetMs: Infinity})).rejects.toThrow('positive integer');
        await expect(roam.replay({repeat: 0})).rejects.toThrow('positive integer');
    } finally {dispose();}
});

// Exercise the worker protocol's cleanup-error outcome deterministically, without
// relying on the operating system denying taskkill or leaving real descendants.
test('Roam aborts reduction immediately after a cleanup-error result and preserves its evidence', async () => {
    const root = mkdtempSync(join(tmpdir(), 'roam-reduce-stop-'));
    try {
        const attempts = join(root, 'attempts.jsonl');
        const source = `import {createInterface} from 'node:readline';
import {appendFileSync} from 'node:fs';
const lines=createInterface({input:process.stdin});
process.stdout.write(JSON.stringify({type:'hello',format:'roam-worker/1',capabilities:[]})+'\\n');
lines.on('line',line=>{
 const message=JSON.parse(line); if(message.type!=='run')return;
 const length=message.case.steps.length; appendFileSync(${JSON.stringify(attempts)},JSON.stringify(length)+'\\n');
 const result=length===4
  ? {status:'fail',steps:0,issues:[],failure:{propertyId:'fixture-value',message:'initial mismatch'}}
  : {status:'error',steps:0,issues:[{phase:'host-cleanup',name:'TreeCleanupError',message:'fixture cleanup refusal'}]};
 process.stdout.write(JSON.stringify({type:'result',result})+'\\n');lines.close();process.stdin.pause();
});`;
        const hash = createHash('sha256').update(source).digest('hex'), directory = join(root, '.snapshots', hash);
        mkdirSync(directory, {recursive: true}); writeFileSync(join(directory, 'driver.mjs'), source);
        const roam = new Roam({root, target: {snapshot: hash}});
        const trace = {format: 'roam/1' as const, contract: 'reduce-stop/1', steps: Array.from({length: 4}, () => ({op: 'step'}))};
        const originalResult = {status: 'fail' as const, steps: 0, issues: [], failure: {propertyId: 'fixture-value', message: 'initial mismatch'}};
        const id = roam.store.publish(trace, originalResult, {entryHash: hash});
        const original = JSON.stringify(roam.store.read(id));
        // Explicit runtime is intentional: this manually authored fixture has no
        // historical runtime identity, while the frozen worker is still verified.
        const reducer = new Roam({root, runtime: 'bun'});
        await expect(reducer.reduce({id, maxRuns: 20})).rejects.toThrow('process tree cleanup failed');
        expect(readFileSync(attempts, 'utf8').trim().split('\n').map(Number)).toEqual([4, 2]);
        expect(roam.store.list()).toHaveLength(2);
        const failureId = roam.store.list().find(value => value !== id)!;
        expect(roam.store.read(failureId).result.issues).toContainEqual({phase: 'host-cleanup', name: 'TreeCleanupError', message: 'fixture cleanup refusal'});
        expect(readdirSync(join(root, id)).some(value => value.startsWith('reduced-'))).toBe(false);
        expect(JSON.stringify(roam.store.read(id))).toBe(original);
    } finally {rmSync(root, {recursive: true, force: true});}
}, 15000);
