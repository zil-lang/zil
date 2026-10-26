/** Command-line adapter. Programmatic workflows live in Roam. */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { FileStore } from './store';
import { decodeCase } from './core';
import { Roam, type RoamOptions } from './roam';

export type CliOptions = Record<string, string>;
export function parseArgs(args: string[]) { const mode = args[0] ?? 'help', options: CliOptions = {}; for (let i = 1; i < args.length; i += 2) { if (!args[i].startsWith('--') || args[i + 1] === undefined || args[i + 1].startsWith('--')) throw new Error(`expected --name value: ${args[i]}`); options[args[i].slice(2)] = args[i + 1]; } return { mode, options }; }
function number(options: CliOptions, name: string, fallback: number) { const value = Number(options[name] ?? fallback); if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`${name} must be a positive integer`); return value; }
export async function main(args = process.argv.slice(2)): Promise<number> {
    const { mode, options: opt } = parseArgs(args);
    if (mode === 'help') { console.log('bun roam/cli.ts gate|explore|replay|reduce|soak [--driver module.ts --build source-id | --snapshot HASH] [--case case.json] [--root test-results/roam] [--cases 100] [--steps 100] [--timeout 10000] [--budget 60000] [--seed 1] [--id evidence-id] [--runtime node|bun]\nReplay/reduce use the stored immutable snapshot by default. Maintenance: recover-capacity or recover-mutex --id ID --root DIR [--token TOKEN --stopped yes]'); return 0; }
    if (mode === 'recover-capacity' || mode === 'recover-mutex') {
        const store = new FileStore(opt.root ?? 'test-results/roam', opt.capacity ? number(opt, 'capacity', 1) : Infinity);
        if (mode === 'recover-mutex' && !opt.id) throw new Error('recover-mutex requires --id');
        if (!opt.token) { console.log(JSON.stringify({ mode, lock: (mode === 'recover-capacity' ? store.capacityLock() : store.mutexLock(opt.id)) ?? null, instruction: 'Stop all writers, then rerun with --token TOKEN --stopped yes' })); return 0; }
        const recovered = mode === 'recover-capacity' ? store.recoverCapacityLock(opt.token, opt.stopped === 'yes') : store.recoverMutex(opt.id, opt.token, opt.stopped === 'yes');
        console.log(JSON.stringify({ mode, recovered })); return 0;
    }
    if (!['gate', 'explore', 'replay', 'reduce', 'soak'].includes(mode)) throw new Error(`unknown mode ${mode}`);
    if (opt.runtime && opt.runtime !== 'node' && opt.runtime !== 'bun') throw new Error('--runtime must be node or bun');
    if (opt.driver && !opt.build) throw new Error('--build required when compiling a new --driver');
    const options: RoamOptions = {
        root: opt.root,
        target: opt.driver ? {driver: opt.driver, build: opt.build} : opt.snapshot ? {snapshot: opt.snapshot, build: opt.build} : undefined,
        runtime: opt.runtime as RoamOptions['runtime'], timeoutMs: number(opt, 'timeout', 10000),
        leaseMs: number(opt, 'lease', 60000), capacity: opt.capacity ? number(opt, 'capacity', 1) : undefined, worker: opt.worker,
    };
    const roam = new Roam(options);
    const budgetMs = number(opt, 'budget', 60000), cases = number(opt, 'cases', 100), steps = number(opt, 'steps', 100), seed = number(opt, 'seed', 1);
    const trace = opt.case ? decodeCase(readFileSync(resolve(opt.case), 'utf8')) : undefined;
    let report;
    if (mode === 'replay') report = await roam.replay({id: opt.id, case: trace, repeat: number(opt, 'repeat', 1), budgetMs});
    else if (mode === 'reduce') {
        if (trace && !opt.id) throw new Error('reduce requires stored evidence');
        report = await roam.reduce({id: opt.id, maxRuns: cases, budgetMs});
    } else if (mode === 'gate') report = await roam.gate({case: trace, cases, steps, seed, budgetMs});
    else if (mode === 'soak') report = await roam.soak({case: trace, steps, seed, budgetMs});
    else report = await roam.explore({case: trace, cases, steps, seed, budgetMs});
    if ('generationFailure' in report) console.error(JSON.stringify(report));
    else console.log(JSON.stringify(report));
    if ('generationFailure' in report) return 2;
    if ('claimed' in report) return 0;
    if ('reduced' in report) return report.reduced.result.status==='fail'? 0:1;
    if ('results' in report) return report.results.every(r => r.status==='pass')? 0:1;
    return report.mode==='gate'&&(Object.entries(report.counts).some(([status,n]) => status!=='pass'&&n>0)||report.stop==='time-budget')? 1:0;
}
if (import.meta.main) main().then(code => { process.exitCode = code; }).catch(error => { console.error(error); process.exitCode = 2; });
