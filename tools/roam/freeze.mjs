import {createHash, randomUUID} from 'node:crypto';
import {copyFile, chmod, mkdir, readFile, readdir, rename, stat, writeFile} from 'node:fs/promises';
import {join, dirname, resolve, relative, isAbsolute} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
export const hashFile = async path => sha256(await readFile(path));
export const writeJson = (path, value) => writeFile(path, JSON.stringify(value, null, 2) + '\n', {flag: 'wx'});
const safe = path => typeof path === 'string' && path !== '' && !isAbsolute(path) && !path.split(/[\\/]/).some(part => part === '..' || part === '');

async function inventory(directory, prefix = '') {
    const found = [];
    for (const entry of await readdir(directory, {withFileTypes: true})) {
        const name = prefix + entry.name;
        if (entry.isDirectory()) found.push(...await inventory(join(directory, entry.name), name + '/'));
        else if (entry.isFile()) found.push(name);
        else throw new Error(`non-regular frozen input: ${name}`);
    }
    return found.sort();
}

export async function verifyVendor(root = repoRoot) {
    const manifest = JSON.parse(await readFile(join(root, 'tools/roam/vendor-manifest.json'), 'utf8'));
    const found = await inventory(join(root, 'tools/roam/vendor'));
    if (JSON.stringify(found) !== JSON.stringify(manifest.files.map(file => file.path).sort())) throw new Error('vendor inventory differs from attachment');
    for (const file of manifest.files) {
        if (!safe(file.path)) throw new Error('unsafe vendor manifest path');
        const bytes = await readFile(join(root, 'tools/roam/vendor', file.path));
        if (bytes.length !== file.bytes || sha256(bytes) !== file.sha256) throw new Error(`vendor changed: ${file.path}`);
    }
    return manifest;
}

export async function freeze(binary, directory, provenance) {
    if (process.platform !== 'linux') throw new Error('this session host currently requires Linux POSIX pipes');
    const vendor = await verifyVendor();
    const paths = [
        'tools/roam/vendor-manifest.json', 'tools/roam/register.mjs', 'tools/roam/pipe-host.sh',
        'tools/roam/freeze.mjs', 'tools/roam/replay.mjs', 'tests/roam/session.mjs', 'tests/roam/worker.mjs',
        'tests/roam/run.mjs', 'crates/zil-driver/examples/roam_session.rs',
        ...vendor.files.map(file => 'tools/roam/vendor/' + file.path),
    ].sort();
    const entries = await Promise.all(paths.map(async path => ({path, bytes: (await stat(join(repoRoot, path))).size, sha256: await hashFile(join(repoRoot, path)), executable: false})));
    entries.push({path: 'bin/roam_session', bytes: (await stat(binary)).size, sha256: await hashFile(binary), executable: true});
    entries.sort((a, b) => a.path.localeCompare(b.path));
    const runtime = {
        node: {version: process.version, sha256: await hashFile(process.execPath)},
        bash: {path: '/bin/bash', sha256: await hashFile('/bin/bash')},
        cat: {path: '/usr/bin/cat', sha256: await hashFile('/usr/bin/cat')},
        platform: process.platform, arch: process.arch,
    };
    const identity = {format: 'zil-roam-snapshot/1', files: entries, runtime, provenance};
    const entryHash = sha256(JSON.stringify(identity));
    await mkdir(directory, {recursive: true});
    const pending = join(directory, '.pending-' + randomUUID());
    await mkdir(pending);
    for (const entry of entries) {
        const target = join(pending, entry.path);
        await mkdir(dirname(target), {recursive: true});
        await copyFile(entry.path === 'bin/roam_session' ? binary : join(repoRoot, entry.path), target);
        await chmod(target, entry.executable ? 0o555 : 0o444);
    }
    await writeJson(join(pending, 'manifest.json'), {...identity, entryHash, boundary: 'Exact source and native bridge bytes; matching Node/bash/cat required. OS, shared libraries and hardware are recorded only by platform/architecture, not frozen.'});
    const destination = join(directory, entryHash);
    await rename(pending, destination);
    return verifySnapshot(destination);
}

export async function verifySnapshot(directory) {
    const manifest = JSON.parse(await readFile(join(directory, 'manifest.json'), 'utf8'));
    const {format, files, runtime, provenance, entryHash} = manifest;
    if (format !== 'zil-roam-snapshot/1' || sha256(JSON.stringify({format, files, runtime, provenance})) !== entryHash) throw new Error('snapshot manifest identity mismatch');
    if (!Array.isArray(files) || JSON.stringify((await inventory(directory)).filter(path => path !== 'manifest.json')) !== JSON.stringify(files.map(file => file.path).sort())) throw new Error('snapshot inventory mismatch');
    for (const file of files) {
        if (!safe(file.path)) throw new Error('unsafe snapshot path');
        const bytes = await readFile(join(directory, file.path));
        if (bytes.length !== file.bytes || sha256(bytes) !== file.sha256) throw new Error(`snapshot hash mismatch: ${file.path}`);
    }
    if (runtime.platform !== process.platform || runtime.arch !== process.arch || runtime.node.version !== process.version || runtime.node.sha256 !== await hashFile(process.execPath)) throw new Error('historical Node runtime mismatch');
    for (const tool of [runtime.bash, runtime.cat]) if (tool.sha256 !== await hashFile(tool.path)) throw new Error(`historical host tool mismatch: ${tool.path}`);
    return {directory: resolve(directory), manifest};
}

export function environment(snapshot, fault, evidenceRoot) {
    const path = relative(evidenceRoot, snapshot.directory).replaceAll('\\', '/');
    if (!safe(path)) throw new Error('snapshot must be inside evidence root');
    return {format: 'zil-session-environment/1', snapshot: path, entryHash: snapshot.manifest.entryHash, fault, transport: 'original-supervise-with-posix-pipes', build: snapshot.manifest.provenance};
}

export async function runFrozen(snapshot, trace, {fault = 'none', store, evidenceRoot, direct = false} = {}) {
    if (!['none', 'stale-after-edit'].includes(fault)) throw new Error('unknown injection');
    await import(pathToFileURL(join(snapshot.directory, 'tools/roam/register.mjs')).href);
    const {supervise} = await import(pathToFileURL(join(snapshot.directory, 'tools/roam/vendor/worker.ts')).href);
    const node = [process.execPath, '--experimental-transform-types', '--import', join(snapshot.directory, 'tools/roam/register.mjs'), join(snapshot.directory, 'tests/roam/worker.mjs'), join(snapshot.directory, 'bin/roam_session'), fault];
    return supervise({
        command: direct ? node[0] : '/bin/bash',
        args: direct ? node.slice(1) : ['--noprofile', '--norc', join(snapshot.directory, 'tools/roam/pipe-host.sh'), ...node],
        cwd: snapshot.directory, trace, timeoutMs: 10_000, maxOutputBytes: 4 * 1024 * 1024,
        store, environment: evidenceRoot ? {...environment(snapshot, fault, evidenceRoot), ...(direct ? {transport: 'original-supervise-direct-probe'} : {})} : undefined,
    });
}
