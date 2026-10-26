import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import {mkdtemp, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {PropertyFailure, execute} from '../../tools/roam/vendor/core.ts';
import {seed as rng} from '../../tools/roam/vendor/data.ts';

export const contract = 'zil-compilation-session/1';
export const initial = () => ({source: 'zero', target: 'native-llvm', cancel: 'none', disk: true, tokens: 1_000_000, nodes: 1_000_000, analysis: 10_000_000});
const choices = {
    edit: ['zero', 'one', 'ct', 'type-error', 'syntax-error'],
    target: ['native-llvm', 'alternate-llvm'],
    cancel: ['none', 'before', 'parse', 'types', 'verify'],
    cache: ['on', 'off', 'corrupt', 'clear'],
};

function equal(propertyId, expected, actual) {
    if (JSON.stringify(expected) !== JSON.stringify(actual)) {
        throw new PropertyFailure(propertyId, propertyId, expected, actual);
    }
}

export function valid(action) {
    if (action.op === 'check') return Object.keys(action).length === 1;
    if (action.op === 'limit') return ['tokens', 'nodes', 'analysis'].includes(action.name) && Number.isSafeInteger(action.value) && action.value >= 0 && action.value <= 10_000_000;
    return choices[action.op]?.includes(action.value) ?? false;
}

export const spec = {
    contract,
    init(value) {
        if (value !== undefined && JSON.stringify(value) !== JSON.stringify(initial())) throw new Error('unsupported initial compiler state');
        return initial();
    },
    valid: (_model, action) => valid(action),
    step(model, action) {
        const next = {...model};
        if (action.op === 'edit') next.source = action.value;
        if (action.op === 'target') next.target = action.value;
        if (action.op === 'cancel') next.cancel = action.value;
        if (action.op === 'limit') next[action.name] = action.value;
        if (action.op === 'cache' && ['on', 'off'].includes(action.value)) next.disk = action.value === 'on';
        return next;
    },
    check(model, observed) {
        equal('session.state', model, observed.state);
        equal('session.no-active-query', 0, observed.active);
        if (observed.kind !== 'check') return;
        equal('session.fresh-no-active-query', 0, observed.cold_active);
        const semantic = result => ({status: result.status, phase: result.phase, diagnostics: result.diagnostics, input: result.input_fingerprint});
        equal('session.warm-cold', semantic(observed.cold), semantic(observed.warm));
        for (const [name, result] of [['warm', observed.warm], ['cold', observed.cold]]) {
            if (result.status === 'Success') equal(`session.verify-${name}`, 1, result.verification_runs);
            for (const event of result.query_events) if (event.phase === 'verify') equal('session.verify-not-cached', 'executed', event.disposition);
        }
        if (model.cancel === 'before') equal('session.cancel-before', 'Cancelled', observed.warm.status);
        if (model.cancel === 'none' && model.tokens === 1_000_000 && model.nodes === 1_000_000 && model.analysis === 10_000_000) {
            equal('session.fixture-category', model.source.endsWith('error') ? 'SafeReject' : 'Success', observed.warm.status);
        }
    },
};

function bridge(binary, directory, fault) {
    const child = spawn(binary, [directory, fault], {stdio: ['pipe', 'pipe', 'pipe']});
    let pending, failure, stderr = '', closed = false;
    const lines = createInterface({input: child.stdout});
    const completion = new Promise(resolve => child.once('close', (code, signal) => {
        closed = true;
        if (code !== 0 || signal) failure ??= new Error(`session bridge exit ${code ?? signal}: ${stderr}`);
        pending?.reject(failure ?? new Error('session bridge closed before response'));
        pending = undefined;
        resolve();
    }));
    child.once('error', error => { failure = error; pending?.reject(error); pending = undefined; });
    child.stderr.on('data', bytes => { stderr = (stderr + bytes).slice(-16384); });
    child.stdin.on('error', error => { failure = error; pending?.reject(error); pending = undefined; });
    lines.on('line', line => {
        try {
            if (!pending) throw new Error('unsolicited session response');
            const value = JSON.parse(line);
            if (value.error) throw new Error(value.error);
            pending.resolve(value); pending = undefined;
        } catch (error) { failure = error; pending?.reject(error); pending = undefined; child.kill('SIGKILL'); }
    });
    return {
        request(command) {
            if (failure) return Promise.reject(failure);
            if (closed || pending) return Promise.reject(new Error('session bridge unavailable'));
            return new Promise((resolve, reject) => { pending = {resolve, reject}; child.stdin.write(`${command}\n`); });
        },
        async close() {
            child.stdin.end();
            const timer = setTimeout(() => child.kill('SIGKILL'), 2000);
            try { await completion; } finally { clearTimeout(timer); lines.close(); }
            if (failure) throw failure;
        },
    };
}

export function driver(binary, fault = 'none') {
    return {
        async init(_value, context) {
            const directory = await mkdtemp(join(tmpdir(), 'zil-roam-session-'));
            context.defer(() => rm(directory, {recursive: true, force: true}));
            const process = bridge(binary, directory, fault);
            context.defer(() => process.close());
            return {process, observed: await process.request('state')};
        },
        async step(real, action) {
            const command = action.op === 'limit' ? `limit\t${action.name}\t${action.value}` : action.op === 'check' ? 'check' : `${action.op}\t${action.value}`;
            real.observed = await real.process.request(command);
        },
        observe: real => real.observed,
    };
}

export function replay(binary, trace, fault = 'none', options = {}) {
    return execute(spec, driver(binary, fault), trace, {history: true, maxSteps: 256, budgetMs: 10_000, ...options});
}

export function generate(seed, steps = 32) {
    const random = rng(seed);
    const operations = [
        {op: 'check'}, {op: 'check'},
        ...Object.entries(choices).flatMap(([op, values]) => values.map(value => ({op, value}))),
        ...['tokens', 'nodes', 'analysis'].flatMap(name => [1, name === 'analysis' ? 10_000_000 : 1_000_000].map(value => ({op: 'limit', name, value}))),
    ];
    const path = Array.from({length: steps}, () => ({...operations[Math.floor(random() * operations.length)]}));
    path.push({op: 'cancel', value: 'none'}, {op: 'limit', name: 'tokens', value: 1_000_000}, {op: 'limit', name: 'nodes', value: 1_000_000}, {op: 'limit', name: 'analysis', value: 10_000_000}, {op: 'edit', value: 'ct'}, {op: 'check'}, {op: 'check'});
    return {format: 'roam/1', contract, seed, steps: path};
}
