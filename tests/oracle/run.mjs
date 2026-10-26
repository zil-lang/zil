import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {execute} from '../../tools/roam/vendor/core.ts';
import {shrink} from '../../tools/roam/vendor/shrink.ts';
import {expected, source, spec, pins, generate, makeCase, mutationCase} from './domain.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const args = process.argv.slice(2);
const options = new Set(['--seed', '--cases', '--compiler', '--out', '--replay', '--require-compiler-sha256']);
for (let i = 0; i < args.length; i += 2) if (!options.has(args[i]) || !args[i + 1] || args[i + 1].startsWith('--')) throw new Error(`Unknown or incomplete option: ${args[i]}`);
function option(name, fallback) { const i = args.indexOf(name); return i < 0 ? fallback : args[i + 1]; }
const seed = Number(option('--seed', '1')), count = Number(option('--cases', '64'));
if (!Number.isInteger(seed) || seed < 0 || seed > 0xffffffff || !Number.isInteger(count) || count < 1 || count > 10000) throw new Error('Invalid bounded seed/case count');
const compiler = path.resolve(option('--compiler', path.join(root, 'target/debug/zil')));
const clang = process.env.ZIL_CLANG || 'clang';
const runId = new Date().toISOString().replaceAll(/[-:.]/g, '') + '-' + crypto.randomBytes(4).toString('hex');
const out = path.resolve(option('--out', path.join(root, 'out/runs/oracle', runId)));
fs.mkdirSync(path.dirname(out), {recursive: true});
fs.mkdirSync(out, {recursive: false});
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const write = (name, value) => fs.writeFileSync(path.join(out, name), JSON.stringify(value, null, 2) + '\n');
function capture(command, folder, timeout) {
    fs.mkdirSync(folder, {recursive: true});
    const stdout = path.join(folder, 'stdout.txt'), stderr = path.join(folder, 'stderr.txt');
    const outFd = fs.openSync(stdout, 'wx'), errFd = fs.openSync(stderr, 'wx');
    let result;
    try { result = spawnSync(command[0], command.slice(1), {cwd: root, stdio: ['ignore', outFd, errFd], timeout}); }
    finally { fs.closeSync(outFd); fs.closeSync(errFd); }
    return {...result, stdout: fs.readFileSync(stdout, 'utf8'), stderr: fs.readFileSync(stderr, 'utf8')};
}
const identity = command => {
    const r = capture(command, path.join(out, `identity-${command[0].replaceAll(/[^a-zA-Z0-9_-]/g, '_')}`), 10000);
    if (r.error || r.status !== 0) throw new Error(`Identity command failed: ${command.join(' ')}: ${r.error?.message ?? r.stderr}`);
    return {command, status: r.status, stdout: r.stdout, stderr: r.stderr};
};
let invocations = 0;
let header = {schema_version: 1, contract: spec.contract, run_id: runId};
const results = [];
async function main() {
header = {...header, source_commit: identity(['git', 'rev-parse', 'HEAD']).stdout.trim(), seed, scope: 'Independent bounded BigInt semantic oracle against real ZIL source/native; no resource or full-language proof', compiler: {path: compiler, sha256: sha(fs.readFileSync(compiler))}, clang: identity([clang, '--version']), node: process.version, platform: `${process.platform}-${process.arch}`, model_sha256: sha(fs.readFileSync(new URL('./domain.mjs', import.meta.url))), driver_sha256: sha(fs.readFileSync(fileURLToPath(import.meta.url))), vendor_manifest_sha256: sha(fs.readFileSync(path.join(root, 'tools/roam/vendor-manifest.json'))), runtime_closure: 'Compiler binary identity and clang version recorded; shared libraries and OS are not a hermetic frozen runtime.', replay_policy: 'Replay uses the supplied current compiler; --require-compiler-sha256 rejects binary identity drift. No claim of fully frozen runtime replay.'};
write('identity.json', header);
const requiredHash = option('--require-compiler-sha256', null);
if (requiredHash !== null && (!/^[0-9a-f]{64}$/.test(requiredHash) || requiredHash !== header.compiler.sha256)) throw new Error('Compiler identity differs from the required SHA-256');
function driver(mutation = false) {
    return {
        init: () => ({observed: null}),
        step(real, action) {
            const folder = path.join(out, `native-${String(++invocations).padStart(5, '0')}`); fs.mkdirSync(folder);
            const input = path.join(folder, 'input.zil'), executable = path.join(folder, 'program');
            fs.writeFileSync(input, source(action.program, mutation));
            const command = [compiler, 'build', input, '--entry', 'value', '--output', executable, '--json'];
            const build = capture(command, path.join(folder, 'compile'), 15000);
            fs.writeFileSync(path.join(folder, 'compile.json'), JSON.stringify({command, status: build.status, signal: build.signal, stdout: build.stdout, stderr: build.stderr, error: build.error?.message}, null, 2));
            if (build.error) throw build.error;
            let report;
            try { report = JSON.parse(build.stdout); } catch { throw new Error('Compiler did not return JSON'); }
            if (build.status === 1 && report.status === 'SafeReject' && report.phase === 'ct' && report.diagnostics.some(d => d.phase === 'ct' && d.code === 'E-CT-ARITHMETIC')) { real.observed = {kind: 'ct-reject'}; return; }
            if (build.status !== 0 || report.status !== 'Success') { real.observed = {kind: 'compile-result', category: report.status, diagnostics: report.diagnostics}; return; }
            const run = capture([executable], path.join(folder, 'run'), 3000);
            fs.writeFileSync(path.join(folder, 'run.json'), JSON.stringify({command: [executable], status: run.status, signal: run.signal, stdout: run.stdout, stderr: run.stderr, error: run.error?.message}, null, 2));
            if (run.error) throw run.error;
            if (run.signal === 'SIGILL' && process.platform === 'linux' && process.arch === 'x64') real.observed = {kind: 'trap'};
            else if (run.status === 0 && /^-?\d+\s*$/.test(run.stdout)) real.observed = {kind: 'value', value: BigInt(run.stdout.trim()).toString()};
            else real.observed = {kind: 'unexpected-runtime', status: run.status, signal: run.signal, stdout: run.stdout};
        },
        observe: real => real.observed,
    };
}

const replayPath = option('--replay', null);
const cases = replayPath ? [{id: 'replay', input: JSON.parse(fs.readFileSync(replayPath, 'utf8'))}] : [
    {id: 'boundary-pins', input: makeCase(pins())},
    {id: 'seeded-path', input: makeCase(generate(seed, count), seed)},
];
for (const item of cases) {
    write(`${item.id}.case.json`, item.input);
    if (Array.isArray(item.input?.steps)) write(`${item.id}.expected.json`, item.input.steps.map(action => {
        try { return expected(action?.program); } catch (error) { return {invalid: true, reason: error.message}; }
    }));
    const result = await execute(spec, driver(), item.input, {history: true, budgetMs: 180000});
    write(`${item.id}.result.json`, result);
    results.push({id: item.id, status: result.status, steps: result.steps, planned_steps: item.input?.steps?.length ?? 0, case_path: `${item.id}.case.json`, result_path: `${item.id}.result.json`});
    if (result.status === 'fail') {
        const reduced = await shrink(item.input, input => execute(spec, driver(), input, {history: true, budgetMs: 30000}), {maxRuns: 100, budgetMs: 60000});
        write(`${item.id}.reduction.json`, reduced); write(`${item.id}.reduced.case.json`, reduced.reduced);
    }
}

let detection = null;
if (!replayPath) {
    const original = mutationCase();
    const replay = input => execute(spec, driver(true), input, {history: true, budgetMs: 30000});
    const found = await replay(original), repeated = await replay(original);
    const reduced = await shrink(original, replay, {maxRuns: 20, budgetMs: 30000});
    const correct = await execute(spec, driver(), reduced.reduced, {history: true, budgetMs: 30000});
    detection = {scope: 'Harness mutation test only: source emitter deliberately changes subtraction to addition; production compiler is unchanged', passed: found.status === 'fail' && repeated.status === 'fail' && found.failure?.propertyId === repeated.failure?.propertyId && reduced.result.status === 'fail' && reduced.reduced.steps.length === 1 && correct.status === 'pass', original, found, repeated, reduced, unmutated_control: correct};
    write('mutation-detection.json', detection);
}
const report = {...header, status: results.every(r => r.status === 'pass' && r.steps > 0 && r.steps === r.planned_steps) && (detection === null || detection.passed) ? 'Pass' : 'Fail', cases: results, executed: results.reduce((sum, r) => sum + r.steps, 0), native_attempts_including_reduction: invocations, mutation_detection_passed: detection?.passed ?? null, result_path: path.relative(root, path.join(out, 'result.json'))};
write('result.json', report);
console.log(JSON.stringify({status: report.status, run_id: runId, executed: report.executed, mutation_detection_passed: report.mutation_detection_passed, result_path: report.result_path}));
process.exitCode = report.status === 'Pass' ? 0 : 1;
}
try { await main(); }
catch (error) {
    const report = {...header, status: 'Error', executed: results.reduce((sum, r) => sum + r.steps, 0), cases: results, native_attempts_including_reduction: invocations, error: {name: error.name, message: error.message}, result_path: path.relative(root, path.join(out, 'result.json'))};
    write('result.json', report); console.error(JSON.stringify(report)); process.exitCode = 1;
}
