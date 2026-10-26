/** Browser lifecycle for a frozen PW worker. The generated entry only supplies dependencies. */
import type {Browser, BrowserContext, BrowserServer, BrowserType, Page} from '@playwright/test';
import {createHash, randomUUID} from 'node:crypto';
import {existsSync, mkdirSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {dirname, join, relative, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {serveGenerator, serveWorker} from '../worker';
import type {RoamCase, RoamDriver, RoamSpec} from '../core';
import {executePw, type PwFixture, type PwOptions, type PwResult} from './pw';

interface PwDomain {
    spec: RoamSpec<any>;
    createDriver(page: Page, url: string): RoamDriver<any>;
    generate(seed: number, steps: number): RoamCase | Promise<RoamCase>;
}
interface PwWorkerOptions {
    moduleUrl: string;
    target: PwFixture & {id: string};
    caseMs: number;
    profile: NonNullable<PwOptions['profile']>;
    channel?: string;
    peer: {manifest: string; module: string; hash: string};
}

export async function servePwWorker(domain: PwDomain, chromium: BrowserType, options: PwWorkerOptions) {
    const {target, caseMs, profile, peer} = options;
    const peerHash = createHash('sha256').update(readFileSync(peer.manifest)).update(readFileSync(peer.module)).digest('hex');
    if (peerHash !== peer.hash) throw new Error('Playwright host changed since snapshot');
    if (process.argv[2] === 'generate') return serveGenerator(domain.generate);
    const root = resolve(dirname(fileURLToPath(options.moduleUrl)), '../..');
    return serveWorker(async (trace, beforeStep, afterStep) => {
        const directory = join(root, '.pw-attempts', randomUUID()), tracePath = join(directory, 'trace.zip');
        mkdirSync(directory, {recursive: true});
        writeFileSync(join(directory, 'case.json'), JSON.stringify(trace));
        let browser: Browser | undefined, server: BrowserServer | undefined, context: BrowserContext | undefined;
        let timer: ReturnType<typeof setTimeout> | undefined, timedOut = false, result: PwResult | undefined;
        try {
            server = await chromium.launchServer({channel: options.channel, timeout: caseMs});
            writeFileSync(join(directory, 'browser.json'), JSON.stringify({pid: server.process().pid, worker: process.pid}));
            const ownedServer = server;
            timer = setTimeout(async () => {
                timedOut = true;
                if (context) await Promise.race([
                    context.tracing.stop({path: tracePath}).catch(() => {}),
                    new Promise(done => setTimeout(done, 200)),
                ]);
                if (browser) void browser.close().catch(() => {});
                await ownedServer.kill().catch(() => {});
            }, caseMs);
            browser = await chromium.connect(server.wsEndpoint(), {timeout: caseMs});
            context = await browser.newContext();
            const page = await context.newPage();
            page.setDefaultTimeout(Math.min(caseMs, 3000));
            const base = new URL(target.url);
            await page.route('**/*', route => {
                const url = new URL(route.request().url());
                if (url.origin !== base.origin) return route.fulfill({status: 404, body: 'outside frozen target'});
                if (url.pathname === base.pathname) return route.fulfill({contentType: 'text/html', body: target.html});
                if (url.pathname === '/fixture.js') return route.fulfill({contentType: 'text/javascript', body: target.code});
                return route.fulfill({status: 404, body: 'outside frozen target'});
            });
            const driver = domain.createDriver(page, target.url), step = driver.step.bind(driver);
            driver.step = async (real, action, context) => {await step(real, action, context); afterStep(context.step);};
            result = await executePw(domain.spec, driver, trace, {
                page, build: {id: target.id, frozen: true, source: relative(root, fileURLToPath(options.moduleUrl))},
                driverVersion: trace.contract + '/pw/1', profile, tracePath, history: true, beforeStep,
            });
        } finally {
            const failed = (error: unknown) => {
                if (!result) throw error;
                result.issues.push({phase: 'pw-browser-close', name: 'CleanupError', message: String(error)});
                if (result.status === 'pass') result.status = 'error';
            };
            try {if (browser) await browser.close();}
            catch (error) {failed(error);}
            finally {
                try {if (server) await server.close();}
                catch (error) {failed(error);}
                finally {clearTimeout(timer);}
            }
        }
        if (timedOut) {
            result.issues.push({phase: 'pw-host', name: 'Timeout', message: 'PW case deadline closed the browser'});
            if (result.status === 'pass') {result.status = 'error'; result.reason = 'pw-time-budget';}
        }
        result.evidence.browserPid = server.process().pid;
        if (existsSync(tracePath) && result.status !== 'pass') result.evidence.trace = tracePath;
        if (result.evidence.trace) result.evidence.trace = relative(root, result.evidence.trace);
        writeFileSync(join(directory, 'report.json'), JSON.stringify(result));
        if (result.status === 'pass') rmSync(directory, {recursive: true, force: true});
        return result;
    }, profile === 'editor-api' ? ['editor-api'] : ['pw-real-input', 'fixture-delivery']);
}
