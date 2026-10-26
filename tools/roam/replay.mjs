import {readFile} from 'node:fs/promises';
import {resolve, join, relative, isAbsolute} from 'node:path';
import {pathToFileURL} from 'node:url';
import {FileStore} from './vendor/store.ts';
import {runFrozen, verifySnapshot} from './freeze.mjs';

export async function historicalReplay(root, id, {clean = false, reduced} = {}) {
    root = resolve(root);
    const original = new FileStore(join(root, 'corpus')).read(id);
    if (original.environment?.format !== 'zil-session-environment/1') throw new Error('unsupported evidence environment');
    const snapshotPath = resolve(root, original.environment.snapshot);
    const rel = relative(root, snapshotPath);
    if (isAbsolute(rel) || rel === '..' || rel.startsWith('../')) throw new Error('snapshot path escapes evidence');
    const snapshot = await verifySnapshot(snapshotPath);
    if (snapshot.manifest.entryHash !== original.environment.entryHash) throw new Error('evidence snapshot hash mismatch');
    let trace = original.trace;
    if (reduced !== undefined) {
        if (!/^reduced-[a-zA-Z0-9-]+\.json$/.test(reduced)) throw new Error('invalid reduced evidence name');
        trace = JSON.parse(await readFile(join(root, 'corpus', id, reduced), 'utf8')).case;
    }
    return runFrozen(snapshot, trace, {fault: clean ? 'none' : original.environment.fault});
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
    const [, , root, id, ...flags] = process.argv;
    if (!root || !id || flags.some(value => value !== '--clean' && !value.startsWith('--reduced='))) throw new Error('usage: replay.mjs <run-directory> <evidence-id> [--clean] [--reduced=reduced-ID.json]');
    const result = await historicalReplay(root, id, {clean: flags.includes('--clean'), reduced: flags.find(value => value.startsWith('--reduced='))?.slice('--reduced='.length)});
    console.log(JSON.stringify({status: result.result.status, steps: result.result.steps, failure: result.result.failure, issues: result.result.issues, journal: result.journal.length}, null, 2));
    // A reproduced counterexample is a real failure, including deliberately injected ones.
    process.exitCode = result.result.status === 'pass' ? 0 : result.result.status === 'fail' ? 1 : 2;
}
