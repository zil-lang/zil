import {test} from 'node:test';
import assert from 'node:assert/strict';
import {existsSync, mkdtempSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import type {Page} from '@playwright/test';
import {executePw, freezePw} from '../plugins/pw';
import {PropertyFailure, type RoamDriver, type RoamSpec} from '../core';
import type {RoamCase} from '../core';

function fakePage(hooks: {stop?(path?: string): void; close?(): void} = {}) {
    const listeners = new Map<string, Set<(value: any) => void>>();
    const traces: any[] = []; let closed = 0;
    const context = {pages: () => [page], browser: () => null, close: async () => {closed++; hooks.close?.();},
        tracing: {start: async (opts: any) => {traces.push(['start', opts]);}, stop: async (opts?: any) => {traces.push(['stop', opts]); hooks.stop?.(opts?.path);}}};
    const page = {context: () => context, url: () => 'about:blank',
        on: (name: string, fn: (value: any) => void) => {if (!listeners.has(name)) listeners.set(name, new Set()); listeners.get(name)!.add(fn);},
        off: (name: string, fn: (value: any) => void) => listeners.get(name)?.delete(fn)};
    return {page: page as unknown as Page, traces, closed: () => closed,
        emit: (name: string, value: any) => {for (const fn of listeners.get(name) ?? []) fn(value);},
        listenerCount: () => [...listeners.values()].reduce((n, set) => n + set.size, 0)};
}
const input: RoamCase = {format: 'roam/1', contract: 'pw-host/1', steps: [{op: 'act'}]};
const spec: RoamSpec<number> = {contract: input.contract, init: () => 0, step: n => n + 1};
const opts = (page: Page) => ({page, build: {id: 'immutable-bundle', frozen: true as const}, driverVersion: 'test/1', tracePath: 'first-failure.zip', history: true});

test('PW preserves first-attempt property evidence, frozen build and disposal without retry', async () => {
    const host = fakePage(); let calls = 0, disposed = 0;
    const build = {id: 'immutable-bundle', frozen: true as const};
    const driver: RoamDriver<{}> = {init: () => ({}), step: () => {calls++; build.id = 'changed'; throw new PropertyFailure('target.invariant', 'violated');}, dispose: () => {disposed++;}};
    const result = await executePw(spec, driver, input, {...opts(host.page), build});
    assert.equal(result.status, 'fail'); assert.equal(result.failure?.propertyId, 'target.invariant');
    assert.equal(calls, 1); assert.equal(disposed, 1); assert.equal(result.environment.build.id, 'immutable-bundle');
    assert.equal(result.evidence.trace, 'first-failure.zip'); assert.equal(host.traces[1][1].path, 'first-failure.zip');
    assert.equal(host.closed(), 1); assert.equal(host.listenerCount(), 0);
});
test('PW pageerror becomes a named failure, and successful cases discard their traces', async () => {
    const broken = fakePage();
    const failed = await executePw(spec, {init: () => ({}), step: () => broken.emit('pageerror', new Error('browser exploded'))}, input, opts(broken.page));
    assert.equal(failed.status, 'fail'); assert.equal(failed.failure?.propertyId, 'pw.pageerror');
    assert.deepEqual(failed.evidence.pageErrors, ['browser exploded']);
    const clean = fakePage();
    const passed = await executePw(spec, {init: () => ({}), step: () => {}}, input, opts(clean.page));
    assert.equal(passed.status, 'pass'); assert.equal(passed.evidence.trace, undefined); assert.equal(clean.traces[1][1].path, 'first-failure.zip');
    const repeat = await executePw(spec, {init: () => ({}), step: () => {}}, input, opts(clean.page));
    assert.equal(repeat.status, 'invalid');
});
test('PW partial initialization records a trace and releases its context', async () => {
    const host = fakePage(); let released = 0;
    const result = await executePw(spec, {init: (_i, ctx) => {ctx.defer(() => {released++;}); throw new Error('init failed');}, step: () => {}}, input, opts(host.page));
    assert.equal(result.status, 'error'); assert.equal(released, 1); assert.equal(host.closed(), 1);
    assert.equal(result.evidence.trace, 'first-failure.zip'); assert.equal(host.listenerCount(), 0);
});
test('PW catches pageerror from driver disposal after the final observation', async () => {
    const host = fakePage();
    const result = await executePw(spec, {init: () => ({}), step: () => {}, dispose: () => host.emit('pageerror', new Error('dispose pageerror'))}, input, opts(host.page));
    assert.equal(result.status, 'fail'); assert.equal(result.failure?.propertyId, 'pw.pageerror');
    assert.equal(result.evidence.trace, 'first-failure.zip');
    assert.ok(result.issues.some(issue => issue.name === 'PageError' && issue.message === 'dispose pageerror'));
    assert.equal(host.closed(), 1); assert.equal(host.listenerCount(), 0);
});
test('PW labels explicit API bridges separately and rejects an undeclared API profile', async () => {
    const api = fakePage();
    const result = await executePw(spec, {capabilities: ['editor-api'], init: () => ({}), step: () => {}}, input, {...opts(api.page), profile: 'editor-api'});
    assert.equal(result.status, 'pass'); assert.equal(result.environment.profile, 'editor-api');
    const missing = fakePage();
    const rejected = await executePw(spec, {init: () => ({}), step: () => {}}, input, {...opts(missing.page), profile: 'editor-api'});
    assert.equal(rejected.status, 'invalid'); assert.equal(missing.traces.length, 0);
});
test('PW retains the primary property failure and appends disposal pageerror evidence', async () => {
    const host = fakePage();
    const result = await executePw(spec, {init: () => ({}), step: () => {throw new PropertyFailure('target.primary', 'original');},
        dispose: () => host.emit('pageerror', new Error('secondary disposal'))}, input, opts(host.page));
    assert.equal(result.status, 'fail'); assert.equal(result.failure?.propertyId, 'target.primary');
    assert.equal(result.failure?.message, 'original'); assert.equal(result.evidence.trace, 'first-failure.zip');
    assert.ok(result.issues.some(issue => issue.name === 'PageError' && issue.message === 'secondary disposal'));
});
test('PW retains errors through trace/context closure and discards only final successful recordings', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'roam-pw-lifecycle-'));
    try {
        const tracePath = join(dir, 'first.zip');
        const late = fakePage({stop: path => {writeFileSync(path!, 'trace'); late.emit('pageerror', new Error('trace closing'));},
            close: () => late.emit('pageerror', new Error('context closing'))});
        const failed = await executePw(spec, {init: () => ({}), step: () => {}}, input, {...opts(late.page), tracePath});
        assert.equal(failed.status, 'fail'); assert.equal(failed.failure?.propertyId, 'pw.pageerror');
        assert.deepEqual(failed.failure?.actual, ['trace closing', 'context closing']);
        assert.equal(failed.evidence.trace, tracePath); assert.equal(existsSync(tracePath), true); assert.equal(late.listenerCount(), 0);
        const successPath = join(dir, 'success.zip'), clean = fakePage({stop: path => writeFileSync(path!, 'trace')});
        const passed = await executePw(spec, {init: () => ({}), step: () => {}}, input, {...opts(clean.page), tracePath: successPath});
        assert.equal(passed.status, 'pass'); assert.equal(passed.evidence.trace, undefined); assert.equal(existsSync(successPath), false);
    } finally {rmSync(dir, {recursive: true, force: true});}
});
test('PW snapshots have stable identity and reject computed import/require after compilation', async () => {
    const root = mkdtempSync(join(tmpdir(), 'roam-pw-freeze-')), domain = join(root, 'domain.ts');
    const options = {root, domain, fixture: {url: 'http://fixture.invalid/', html: '<html></html>', code: 'void 0;'}};
    const valid = `export const spec={contract:'snapshot/1'}; export const createDriver=()=>({});`;
    try {
        writeFileSync(domain, valid + `export const generate=()=>({format:'roam/1',contract:'snapshot/1',steps:[]});`);
        const first = await freezePw(options), second = await freezePw(options);
        assert.equal(first.snapshot, second.snapshot);
        writeFileSync(domain, valid + `export const generate=()=>import(globalThis.livePath);`);
        await assert.rejects(() => freezePw(options), /unresolved dynamic module loading/);
        writeFileSync(domain, valid + `export const generate=()=>require(globalThis.livePath);`);
        await assert.rejects(() => freezePw(options), /unresolved dynamic module loading/);
    } finally {rmSync(root, {recursive: true, force: true});}
});
