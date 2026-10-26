import {spawn} from 'node:child_process';
import {randomUUID} from 'node:crypto';
import {open, mkdir, readFile, readdir, writeFile} from 'node:fs/promises';
import {dirname, join, resolve} from 'node:path';
import {FileStore} from '../../tools/roam/vendor/store.ts';
import {bfs} from '../../tools/roam/vendor/search.ts';
import {shrink} from '../../tools/roam/vendor/shrink.ts';
import {freeze, hashFile, repoRoot, runFrozen, verifySnapshot, writeJson} from '../../tools/roam/freeze.mjs';
import {historicalReplay} from '../../tools/roam/replay.mjs';
import {contract, generate} from './session.mjs';

const arguments_ = process.argv.slice(2);
const argument = name => arguments_.find(value => value.startsWith(name + '='))?.slice(name.length + 1);
if (arguments_.some(value => value !== '--allow-dirty' && !['--out=', '--binary='].some(prefix => value.startsWith(prefix)))) throw new Error('usage: run.mjs [--out=DIR] [--binary=FILE] [--allow-dirty]');
const out = resolve(argument('--out') ?? join(repoRoot, 'out/runs/roam-ts', new Date().toISOString().replaceAll(':', '-') + '-' + randomUUID().slice(0, 8)));
await mkdir(dirname(out), {recursive: true});
await mkdir(out, {recursive: false});
await mkdir(join(out, 'checks'));
await mkdir(join(out, 'logs'));
const started = Date.now();
const summary = {format: 'zil-session-gate/1', status: 'error', scope: 'scalar CompilationSession cache transparency; no language-semantic oracle claim', commands: [], checks: [], injection: null, host_probe: null};

async function command(program, args, label) {
    const log = join(out, 'logs', label + '.log');
    const handle = await open(log, 'wx');
    const entry = {program, args, log: 'logs/' + label + '.log'};
    summary.commands.push(entry);
    try {
        await new Promise((accept, reject) => {
            const child = spawn(program, args, {cwd: repoRoot, stdio: ['ignore', handle.fd, handle.fd]});
            child.once('error', reject);
            child.once('close', (code, signal) => code === 0 && !signal ? accept() : reject(new Error(`${program} exit ${code ?? signal}`)));
        });
    } finally { await handle.close(); }
    return (await readFile(log, 'utf8')).trim();
}

const trace = (steps, meta = {}) => ({format: 'roam/1', contract, steps, meta});
const check = () => ({op: 'check'});
const edit = value => ({op: 'edit', value});
const cancel = value => ({op: 'cancel', value});
const target = value => ({op: 'target', value});
const cache = value => ({op: 'cache', value});
const limit = (name, value) => ({op: 'limit', name, value});
const require_ = (condition, message) => { if (!condition) throw new Error(message); };
const brief = result => ({status: result.status, steps: result.steps, failure: result.failure ?? null, issues: result.issues, reason: result.reason ?? null});

function observations(result) {
    const values = (result.history ?? []).filter(event => event.phase === 'record' && event.step >= 0 && event.step < result.steps && event.value?.kind === 'check').map(event => event.value);
    return {
        checks: values.length,
        statuses: [...new Set(values.map(value => value.warm.status))],
        warm_raw_reused: values.flatMap(value => value.warm.query_events).filter(event => event.phase !== 'verify' && event.disposition === 'reused').length,
        warm_verify_executed: values.flatMap(value => value.warm.query_events).filter(event => event.phase === 'verify' && event.disposition === 'executed').length,
        disk: [...new Set(values.map(value => value.warm.disk_cache))],
    };
}

try {
    const sourceCommit = await command('git', ['rev-parse', 'HEAD'], 'source-commit');
    const trackedDirty = await command('git', ['status', '--porcelain', '--untracked-files=no'], 'tracked-dirty');
    const sourceDirty = await command('git', ['status', '--porcelain', '--untracked-files=all', '--', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'build.rs', 'crates', 'tools/roam', 'tests/roam'], 'source-dirty');
    const dirty = (trackedDirty + '\n' + sourceDirty).trim();
    require_(!dirty || arguments_.includes('--allow-dirty'), 'commit implementation before a formal evidence run; --allow-dirty is development only');
    const rustVersion = await command('rustc', ['--version', '--verbose'], 'rust-version');
    let binary = argument('--binary');
    if (binary) binary = resolve(binary);
    else {
        await command('cargo', ['rustc', '--locked', '--offline', '-p', 'zil-driver', '--example', 'roam_session', '--', '-C', 'strip=debuginfo'], 'build');
        binary = join(repoRoot, 'target/debug/examples/roam_session');
    }
    const snapshot = await freeze(binary, join(out, 'snapshots'), {sourceCommit, sourceDirty: Boolean(dirty), rustVersion, build: argument('--binary') ? 'provided native bridge; exact bytes hashed' : 'cargo rustc --locked --offline -p zil-driver --example roam_session -- -C strip=debuginfo'});
    summary.snapshot = {path: 'snapshots/' + snapshot.manifest.entryHash, entryHash: snapshot.manifest.entryHash, sourceCommit, sourceDirty: Boolean(dirty)};
    const store = new FileStore(join(out, 'corpus'));
    let sequence = 0;
    const run = async (testCase, label, fault = 'none', direct = false) => {
        const name = String(sequence++).padStart(4, '0') + '-' + label;
        await writeJson(join(out, 'checks', name + '.case.json'), testCase);
        const result = await runFrozen(snapshot, testCase, {fault, store, evidenceRoot: out, direct});
        await writeFile(join(out, 'checks', name + '.journal.jsonl'), result.journal.join('\n') + (result.journal.length ? '\n' : ''), {flag: 'wx'});
        await writeFile(join(out, 'checks', name + '.stderr.txt'), result.stderr, {flag: 'wx'});
        const record = {name, fault, transport: direct ? 'direct-host-probe' : 'original-supervise-posix-pipes', ...brief(result.result), observations: observations(result.result), evidenceId: result.evidenceId ?? null};
        summary.checks.push(record);
        return {...result, name};
    };

    // Diagnostic host probe is retained even when the environment cannot launch
    // a Node worker on libuv socketpair descriptors. It is never a compiler pass.
    const probe = await run(trace([check()]), 'direct-node-probe', 'none', true);
    summary.host_probe = {name: probe.name, ...brief(probe.result), evidenceId: probe.evidenceId ?? null, production_verdict: 'excluded-host-capability-probe'};

    const fixed = [
        ['reuse', [check(), check()]],
        ['edits', [check(), edit('one'), check(), edit('type-error'), check(), edit('syntax-error'), check(), edit('ct'), check(), check()]],
        ['target', [check(), target('alternate-llvm'), check(), check(), target('native-llvm'), check()]],
        ['disk', [check(), cache('corrupt'), check(), cache('off'), check(), cache('clear'), check(), cache('on'), check(), check()]],
        ...['before', 'parse', 'types', 'verify'].map(value => ['cancel-' + value, [check(), cancel(value), check(), cancel('none'), check(), check()]]),
        ...['tokens', 'nodes', 'analysis'].map(name => ['limit-' + name, [check(), limit(name, 1), check(), limit(name, name === 'analysis' ? 10_000_000 : 1_000_000), check(), check()]]),
    ];
    for (const [label, steps] of fixed) {
        const tested = await run(trace(steps, {campaign: 'fixed', label}), 'fixed-' + label);
        require_(tested.result.status === 'pass', `fixed path ${label}: ${JSON.stringify(brief(tested.result))}`);
        if (label === 'reuse') require_(observations(tested.result).warm_raw_reused >= 5 && observations(tested.result).warm_verify_executed === 2, 'reuse path did not exercise raw reuse and uncached Verify');
    }
    summary.fixed_paths = fixed.length;

    const explore = async (alphabet, depth, fault, label) => {
        let path = [];
        return bfs({init: () => [], ops: () => alphabet, next: (model, action) => [...model, action], key: model => JSON.stringify(model)}, {
            reset: () => { path = []; },
            fire: action => { path.push(action); },
            chk: async () => {
                const actual = await run(trace(path, {campaign: label}), label, fault);
                return actual.result.status === 'pass' ? undefined : {name: actual.name, ...brief(actual.result)};
            },
        }, {depth, cap: 1000, maxBad: 1, tag: label});
    };
    const bounded = await explore([check(), edit('type-error'), target('alternate-llvm'), cancel('before'), limit('tokens', 1), cache('corrupt'), cache('clear')], 2, 'none', 'bounded-bfs');
    summary.bounded_bfs = bounded;
    require_(bounded.fail === 0 && bounded.edges === 56 && bounded.states === 57 && !bounded.capped && bounded.stopReason === 'depth' && bounded.complete === false, 'bounded traversal did not completely cover the declared depth-two prefix set');

    for (const seed of [0, 1, 2, 3, 7, 11, 17, 31, 63, 127, 255, 1024, 65535, 0x12345678, 0xdeadbeef, 0xffffffff]) {
        const tested = await run(generate(seed, 64), 'seed-' + seed);
        require_(tested.result.status === 'pass', `seed ${seed}: ${JSON.stringify(brief(tested.result))}`);
    }
    summary.seeded_paths = {count: 16, generated_actions: 64, recovery_actions: 7, actions_per_path: 71, rng: 'original Roam mulberry32', provenance_only: true};

    const discovered = await explore([check(), edit('type-error'), edit('zero')], 3, 'stale-after-edit', 'injected-bfs');
    require_(discovered.fail === 1 && discovered.bad[0].path.length === 3 && discovered.bad[0].why.failure?.propertyId === 'session.warm-cold', 'bounded traversal failed to discover the declared stale-report injection');
    const originalCase = trace([cache('on'), check(), target('alternate-llvm'), check(), cancel('none'), edit('type-error'), cache('corrupt'), check()], {campaign: 'injected-stale-report', discoveredPath: discovered.bad[0].path, injection: 'test harness only, not a discovered compiler defect'});
    const original = await run(originalCase, 'injected-original', 'stale-after-edit');
    require_(original.result.status === 'fail' && original.result.failure?.propertyId === 'session.warm-cold' && original.evidenceId, 'injected counterexample was not durably published');
    const originalPath = join(out, 'corpus', original.evidenceId, 'case.json');
    const originalHash = await hashFile(originalPath);
    const replayResults = [];
    for (let index = 0; index < 3; index++) {
        const replayed = await historicalReplay(out, original.evidenceId);
        replayResults.push(brief(replayed.result));
        require_(replayed.result.status === 'fail' && replayed.result.failure?.propertyId === 'session.warm-cold', 'frozen replay did not reproduce the original property');
        await writeJson(join(out, 'checks', 'historical-replay-' + index + '.json'), replayed);
    }
    const reduced = await shrink(originalCase, async candidate => (await run(candidate, 'shrink-attempt', 'stale-after-edit')).result, {maxRuns: 64, budgetMs: 30_000});
    require_(reduced.stopped === 'complete' && reduced.result.status === 'fail' && reduced.result.failure?.propertyId === 'session.warm-cold' && reduced.reduced.steps.length === 3, 'original shrink did not finish the expected deletion-minimal witness');
    store.reduced(original.evidenceId, reduced.reduced, reduced.result);
    const reducedName = (await readdir(join(out, 'corpus', original.evidenceId))).find(name => name.startsWith('reduced-'));
    const reducedReplay = await historicalReplay(out, original.evidenceId, {reduced: reducedName});
    require_(reducedReplay.result.status === 'fail' && reducedReplay.result.failure?.propertyId === 'session.warm-cold', 'stored reduced witness did not reproduce');
    const clean = await historicalReplay(out, original.evidenceId, {reduced: reducedName, clean: true});
    require_(clean.result.status === 'pass', 'normal implementation failed the reduced injected witness');
    require_(await hashFile(originalPath) === originalHash, 'reduction changed original evidence');
    await writeJson(join(out, 'shrink.json'), reduced);
    await writeJson(join(out, 'checks', 'reduced-replay.json'), reducedReplay);
    await writeJson(join(out, 'checks', 'clean-control.json'), clean);
    summary.injection = {kind: 'adapter stale report after edit', production_defect: false, discovery: discovered, evidenceId: original.evidenceId, original_sha256: originalHash, original_steps: originalCase.steps.length, replays: replayResults, reduced: reducedName, reduced_steps: reduced.reduced.steps.length, shrink_runs: reduced.runs, shrink_stopped: reduced.stopped, reduced_replay: brief(reducedReplay.result), clean_control: brief(clean.result), original_unchanged: true};
    await verifySnapshot(snapshot.directory);
    summary.status = 'pass';
    summary.boundaries = ['Bounded BFS covers depth <= 2 in a fixed seven-action alphabet; complete remains false for the unbounded system.', 'Injected BFS witness is shortest only within its finite three-action alphabet and declared traversal.', 'Original shrink preserves propertyId and path validity, not identical root cause or global minimality.', 'Warm/cold runs share compiler implementation; semantic correctness requires the independent source oracle.', 'Original Bun freezeDriver and the complete Roam top-level host are not integrated.', 'Exact sources and native bridge are frozen; runtime tools must match, while OS/shared libraries/hardware are not hermetically frozen.', 'The direct Node host probe may be InfraError; only explicit POSIX-pipe supervised campaigns determine this gate.'];
} catch (error) {
    summary.error = {name: error.name, message: error.message};
    process.exitCode = 1;
} finally {
    summary.elapsed_ms = Date.now() - started;
    await writeJson(join(out, 'report.json'), summary);
    console.log(JSON.stringify({status: summary.status, report: join(out, 'report.json'), checks: summary.checks.length, fixed_paths: summary.fixed_paths, bounded_bfs: summary.bounded_bfs && {edges: summary.bounded_bfs.edges, complete: summary.bounded_bfs.complete, stopReason: summary.bounded_bfs.stopReason}, seeded_paths: summary.seeded_paths, injection: summary.injection && {original_steps: summary.injection.original_steps, reduced_steps: summary.injection.reduced_steps, shrink_runs: summary.injection.shrink_runs}, error: summary.error}));
}
