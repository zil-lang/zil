import {test, expect} from 'bun:test';
import {shrink} from '../shrink';
import {execute, PropertyFailure, type RoamSpec, type RoamDriver, type RoamCase} from '../core';
const original: RoamCase = {format: 'roam/1', contract: 'resource/1', steps: [{op: 'noise'}, {op: 'create'}, {op: 'use'}, {op: 'noise'}]};
const spec: RoamSpec<number> = {
    contract: original.contract, init: () => 0,
    dependencies: a => a.op === 'create' ? {provides: ['r']} : a.op === 'use' ? {requires: ['r']} : {},
    step: (m, a) => m + (a.op === 'use' ? 1 : 0),
    check(m) {if (m) throw new PropertyFailure('use-error', 'resource bad');},
};
const driver: RoamDriver<null> = {init: () => null, step() {}};
test('roam-shrink-01 rejects orphan references and preserves original input', async () => {
    const before = JSON.stringify(original);
    const r = await shrink(original, c => execute(spec, driver, c));
    expect(r.reduced.steps.map(a => a.op)).toEqual(['create', 'use']);
    expect(r.result.failure?.propertyId).toBe('use-error'); expect(JSON.stringify(original)).toBe(before);
});
test('roam-shrink-02 other property and infrastructure errors cannot replace failure', async () => {
    const r = await shrink(original, async c => c.steps.length === 4
        ? {status: 'fail', steps: 4, issues: [], failure: {propertyId: 'target', message: 'original'}}
        : {status: 'fail', steps: 0, issues: [], failure: {propertyId: 'different', message: 'wrong'}});
    expect(r.reduced).toEqual(original); expect(r.result.failure?.message).toBe('original');
});
test('roam-shrink-03 budget exits with intact evidence', async () => {
    const r = await shrink(original, c => execute(spec, driver, c), {maxRuns: 1});
    expect(r.stopped).toBe('budget'); expect(r.runs).toBe(1); expect(r.original).toEqual(original);
    const aborted = await shrink(original, c => execute(spec, driver, c), {signal: {aborted: true}});
    expect(aborted.stopped).toBe('aborted'); expect(aborted.runs).toBe(0);
});
test('roam-shrink-04 throwing replay never destroys original failure', async () => {
    let calls = 0;
    const r = await shrink(original, async c => {
        if (calls++) throw Error('worker failed');
        return execute(spec, driver, c);
    });
    expect(r.reduced).toEqual(original); expect(r.result.failure?.propertyId).toBe('use-error');
});
test('roam-shrink-05 final replay time is included when trace is empty', async () => {
    let time = 0;
    const r = await shrink({...original, steps: []}, async () => {
        time = 10; return {status: 'fail', steps: 0, issues: [], failure: {propertyId: 'target', message: 'bad'}};
    }, {now: () => time, budgetMs: 5});
    expect(r.stopped).toBe('budget'); expect(r.result.failure?.propertyId).toBe('target');
});
