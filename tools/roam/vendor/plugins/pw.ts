/** Optional browser host. Playwright is type-only; the caller supplies a fresh Page. */
import type {Page} from '@playwright/test';
import {unlink} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {existsSync, mkdirSync, readFileSync, writeFileSync, unlinkSync} from 'node:fs';
import {dirname, resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
import {execute, PropertyFailure, type ExecuteOptions, type RoamDriver, type RoamSpec} from '../core';
import type {Json, RoamCase, RunResult} from '../core';

export interface PwBuild {id: string; frozen: true; source?: string}
export interface PwOptions extends ExecuteOptions {
    page: Page;
    build: PwBuild;
    driverVersion: string;
    /** API bridges must declare that capability; they do not cover browser text input. */
    profile?: 'pw-real-input' | 'editor-api';
    /** A distinct file for this execution, including the first attempt. */
    tracePath: string;
    /** Default true: the case owns and releases its entire isolated context. */
    closeContext?: boolean;
}
export interface PwResult extends RunResult {
    environment: {build: PwBuild; driverVersion: string; profile: 'pw-real-input' | 'editor-api'; browser: string};
    evidence: {pageErrors: string[]; consoleErrors: string[]; trace?: string; browserPid?: number};
}
const usedContexts = new WeakSet<object>();

export interface PwFixture {url: string; html: string; code: string}
/** Prepare an external browser worker for the existing host CLI --snapshot entry.
 * Own/domain code and target assets are bundled. Playwright's version/entry is pinned;
 * its transitive SDK installation and browser binary remain explicit host dependencies.
 */
export async function freezePw(options: {root: string; domain: string; fixture: PwFixture; profile?: PwOptions['profile']; channel?: string; caseMs?: number}) {
    const runtime = (globalThis as any).Bun;
    const caseMs = options.caseMs ?? 5000;
    if (!Number.isSafeInteger(caseMs) || caseMs < 100) throw new Error('caseMs must be an integer >=100');
    const require = createRequire(import.meta.url), peer = require.resolve('@playwright/test');
    const peerModule = resolve(dirname(peer), 'index.mjs'), peerManifest = resolve(dirname(peer), 'package.json');
    if (!existsSync(peerModule)) throw new Error('Playwright ESM host entry missing');
    const peerHash = createHash('sha256').update(readFileSync(peerManifest)).update(readFileSync(peerModule)).digest('hex');
    const target = {...options.fixture, id: createHash('sha256').update(options.fixture.html).update(options.fixture.code).digest('hex')};
    const snapshots = resolve(options.root, '.snapshots'); mkdirSync(snapshots, {recursive: true});
    const temporary = resolve(snapshots, `pw-${process.pid}-${Date.now()}.ts`), compiled = temporary + '.mjs';
    const modulePath = (value: string) => JSON.stringify(value.replaceAll('\\', '/'));
    const source = `import {chromium} from '__roam_pw_peer__';
import * as domain from ${modulePath(resolve(options.domain))};
import {servePwWorker} from ${modulePath(fileURLToPath(new URL('./pw-worker.ts', import.meta.url)))};
await servePwWorker(domain, chromium, {...${JSON.stringify({target, caseMs, profile: options.profile ?? 'pw-real-input', channel: options.channel, peer: {manifest: peerManifest, module: peerModule, hash: peerHash}})},moduleUrl:import.meta.url});`;
    writeFileSync(temporary, source);
    try {
        let output: string;
        if (runtime?.build) {
            const built = await runtime.build({entrypoints: [temporary], target: 'node', format: 'esm', packages: 'bundle', external: ['__roam_pw_peer__'], minify: false, write: false});
            if (!built.success || built.outputs.length !== 1) throw new Error(`PW snapshot failed: ${built.logs?.map(String).join('\n')}`);
            output = await built.outputs[0].text();
        } else {
            const built = spawnSync(process.env.BUN_EXEC_PATH || 'bun', ['build', temporary, '--target=node', '--format=esm', '--external', '__roam_pw_peer__', '--outfile', compiled], {encoding: 'utf8', timeout: 30000, windowsHide: true});
            if (built.error || built.status !== 0) throw new Error(`PW snapshot compiler: ${built.error || built.stderr}`);
            output = readFileSync(compiled, 'utf8');
        }
        output = output.replace(/pw-\d+-\d+\.ts/g, 'roam-pw-entry.ts').replace(/(['"])__roam_pw_peer__\1/g, JSON.stringify(pathToFileURL(peerModule).href));
        if (/\bimport\s*\((?!\s*['"])/.test(output) || /(?<![\w$])(?:__require\d*|require)\s*\((?!\s*['"])/.test(output)) throw new Error('unresolved dynamic module loading is unsupported in a frozen PW driver');
        const hash = createHash('sha256').update(output).digest('hex');
        const directory = resolve(snapshots, hash); mkdirSync(directory, {recursive: true});
        const file = resolve(directory, 'driver.mjs');
        if (!existsSync(file)) writeFileSync(file, output, {flag: 'wx'});
        else if (readFileSync(file, 'utf8') !== output) throw new Error('PW snapshot identity collision');
        return {snapshot: hash, fixture: target.id, root: resolve(options.root), caseMs, minimumHostTimeoutMs: caseMs * 2 + 2000};
    } finally {unlinkSync(temporary); if (existsSync(compiled)) unlinkSync(compiled);}
}

export async function executePw<M, R>(spec: RoamSpec<M>, driver: RoamDriver<R>, input: RoamCase, options: PwOptions): Promise<PwResult> {
    const {page, tracePath} = options, context = page.context();
    const build = Object.freeze({...options.build});
    const environment = Object.freeze({build, driverVersion: options.driverVersion, profile: options.profile ?? 'pw-real-input',
        browser: context.browser()?.version() ?? 'injected'});
    const evidence: PwResult['evidence'] = {pageErrors: [], consoleErrors: []};
    const reject = (message: string): PwResult => ({status: 'invalid', steps: 0, issues: [{phase: 'pw-context', name: 'InvalidContext', message}], environment, evidence});
    if (!build.id || build.frozen !== true || !tracePath || !options.driverVersion) return reject('Frozen build, driver version and first-attempt trace path are required');
    if (environment.profile === 'editor-api' && !driver.capabilities?.includes('editor-api')) return reject('editor-api profile requires the explicit editor-api driver capability');
    if (usedContexts.has(context) || context.pages().length !== 1 || page.url() !== 'about:blank') return reject('Each case requires a fresh isolated context and its blank Page');
    usedContexts.add(context);
    const onError = (error: Error) => evidence.pageErrors.push(error.message);
    const onConsole = (message: {type(): string; text(): string}) => {if (message.type() === 'error') evidence.consoleErrors.push(message.text());};
    page.on('pageerror', onError); page.on('console', onConsole);
    let tracing = false, result: RunResult;
    const checkErrors = () => {
        if (evidence.pageErrors.length) throw new PropertyFailure('pw.pageerror', 'Uncaught browser error', [], [...evidence.pageErrors]);
    };
    const wrapped: RoamDriver<R> = {
        ...driver,
        init: async (initial, ctx) => {ctx.record({kind: 'pw-environment', ...environment} as Json); return driver.init(initial, ctx);},
        observe: async (real, ctx) => {checkErrors(); const observed = await driver.observe?.(real, ctx); checkErrors(); return observed ?? null;},
        done: async (real, ctx) => {await driver.done?.(real, ctx); checkErrors();},
    };
    try {
        await context.tracing.start({screenshots: true, snapshots: true, sources: true}); tracing = true;
        result = await execute(spec, wrapped, input, options);
    } catch (error) {
        result = {status: 'error', steps: 0, issues: [{phase: 'pw-host', name: 'HostError', message: String(error)}]};
    }
    let notedErrors = 0, traceSaved = false;
    const includePageErrors = () => {
        for (const message of evidence.pageErrors.slice(notedErrors)) result.issues.push({phase: 'pw-pageerror', step: Math.max(-1, result.steps - 1), name: 'PageError', message});
        notedErrors = evidence.pageErrors.length;
        if (!notedErrors) return;
        if (!result.failure) {
            result.status = 'fail';
            result.failure = {propertyId: 'pw.pageerror', message: 'Uncaught browser error', expected: [], actual: [...evidence.pageErrors]};
        } else if (result.failure.propertyId === 'pw.pageerror') result.failure.actual = [...evidence.pageErrors];
    };
    // execute() also runs driver.dispose/deferred cleanup. Errors from those phases must
    // be included before deciding whether to retain the first-attempt recording.
    includePageErrors();
    try {
        if (tracing) {
            // Export provisionally: stop/close can themselves dispatch a final pageerror.
            // A successful case is discarded only after both lifecycle boundaries close.
            await context.tracing.stop({path: tracePath}); traceSaved = true;
        }
    } catch (error) {
        result.issues.push({phase: 'pw-trace', name: 'EvidenceError', message: String(error)});
        if (result.status === 'pass') result.status = 'error';
    } finally {
        includePageErrors();
        if (options.closeContext !== false) try {await context.close();} catch (error) {
            result.issues.push({phase: 'pw-dispose', name: 'CleanupError', message: String(error)});
            if (result.status === 'pass') result.status = 'error';
        }
        page.off('pageerror', onError); page.off('console', onConsole);
    }
    includePageErrors();
    if (traceSaved && result.status === 'pass') try {await unlink(tracePath);} catch (error: any) {
        if (error.code !== 'ENOENT') {
            result.issues.push({phase: 'pw-trace-discard', name: 'EvidenceError', message: String(error)});
            result.status = 'error';
        }
    }
    if (traceSaved && result.status !== 'pass') evidence.trace = tracePath;
    return {...result, environment, evidence};
}
