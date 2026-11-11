"""Frozen research support; importing this module never launches a process."""
import hashlib
import fcntl
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent / 'vendor'))
from runtime_gate import stats
from processes import measured as measure_process
from runtime_fixture import generate_runtime
from fixture import generate

ROOT = Path(__file__).resolve().parent
MAIN = '99035931d358add0eaed6d4535035c38413701dd'
EXPORTER = '2549cbec8e1f3096d4c8c8a960459c83095ad13d'
ORDERS = ['ABR', 'BRA', 'RAB', 'ARB', 'RBA', 'BAR']
OWN_GROUP = True
_IDENTITY_CAPTURE = None
_IDENTITY_CAPTURE_PATHS = set()
_REQUEST_IDENTITIES = None

def measured(command, log, timeout, data=None):
    return measure_process(command, log, timeout, data, own_group=OWN_GROUP)

def sha(path):
    name = os.path.abspath(path)
    if _REQUEST_IDENTITIES is not None and name in _REQUEST_IDENTITIES['files']:
        record = _REQUEST_IDENTITIES['files'][name]
        if _file_stamp(name) != record['stat']:
            raise ValueError('fixed tool changed within compiler request: ' + name)
        return record['sha256']
    # The first request validation deliberately retains the original full reads,
    # including repeated compiler aliases. Only subsequent wrappers use stamps.
    if _IDENTITY_CAPTURE is not None and name in _IDENTITY_CAPTURE_PATHS:
        before = _file_stamp(name)
        digest = hashlib.sha256(Path(path).read_bytes()).hexdigest()
        after = _file_stamp(name)
        if before != after:
            raise ValueError('fixed tool changed while hashing: ' + name)
        previous = _IDENTITY_CAPTURE['files'].get(name)
        record = {'sha256': digest, 'stat': after}
        if previous is not None and previous != record:
            raise ValueError('fixed tool changed during complete validation: ' + name)
        _IDENTITY_CAPTURE['files'][name] = record
        _IDENTITY_CAPTURE['full_hash_calls'] += 1
        _IDENTITY_CAPTURE['full_hash_bytes'] += before['size']
        return digest
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def _file_stamp(path):
    path = Path(os.path.abspath(path))
    link, target = path.lstat(), path.stat()
    return {'path': str(path), 'resolved': str(path.resolve(strict=True)),
            'dev': target.st_dev, 'ino': target.st_ino, 'mode': target.st_mode,
            'size': target.st_size, 'mtime_ns': target.st_mtime_ns, 'ctime_ns': target.st_ctime_ns,
            'link_dev': link.st_dev, 'link_ino': link.st_ino, 'link_mode': link.st_mode,
            'link_size': link.st_size, 'link_mtime_ns': link.st_mtime_ns, 'link_ctime_ns': link.st_ctime_ns}

def _tool_paths(p):
    # Never register source/IR/BC/proof/output files or broad directory prefixes.
    # These exact fixed images are the only SHA reads replaced by stat guards.
    if set(p['tools']) != {'baseline', 'exporter', 'rustc', 'clang', 'lld',
                          'opt', 'bcanalyzer', 'dis', 'plugin'}:
        raise ValueError('only the nine frozen tool roles may reuse image identities')
    paths = {os.path.abspath(v['path']) for v in p['tools'].values()}
    binding = json.loads(Path(p['plugin_build_receipt']['path']).read_text())
    paths.add(os.path.abspath(binding['compiler']['path']))
    return paths

def _guard_fixed_tools():
    if _REQUEST_IDENTITIES is None:
        return
    for name, record in _REQUEST_IDENTITIES['files'].items():
        if _file_stamp(name) != record['stat']:
            raise ValueError('fixed tool stat guard failed: ' + name)

def validate_request(p):
    """One full validate INSIDE this B request, then exact installed-file stamps.

    This is tool identity reuse only: no VerifiedModule, IR or native work is
    cached. The fresh Rust receipt and every original verification still run.
    """
    global _IDENTITY_CAPTURE, _IDENTITY_CAPTURE_PATHS, _REQUEST_IDENTITIES
    if OWN_GROUP:
        raise ValueError('request identity reuse is only for the Clang wrapper')
    # A reused Python interpreter must not carry a prior request's fast path
    # into the next request's first complete validation.
    _REQUEST_IDENTITIES = None
    protocol = Path(os.environ['ZIL_THINLTO_PROTOCOL'])
    protocol_sha = hashlib.sha256(protocol.read_bytes()).hexdigest()
    if protocol_sha != os.environ['ZIL_THINLTO_PROTOCOL_SHA'] or json.loads(protocol.read_text()) != p:
        raise ValueError('actual request protocol changed')
    machine = Path(os.environ['ZIL_RESEARCH_THINLTO_RECEIPT']).absolute()
    context = {'protocol_sha256': protocol_sha, 'machine_receipt_path': str(machine),
               'machine_receipt_sha256': hashlib.sha256(machine.read_bytes()).hexdigest()}
    logroot = Path(os.environ['ZIL_THINLTO_LOGDIR']).absolute()
    if not logroot.is_absolute():
        raise ValueError('request log directory must be absolute')
    logroot.mkdir(parents=True, exist_ok=True)
    stamp = logroot / 'request-tool-identities.json'
    paths = _tool_paths(p)
    fd = os.open(logroot / '.request-tool-identities.lock',
                 os.O_CREAT | os.O_RDWR | os.O_CLOEXEC | os.O_NOFOLLOW, 0o600)
    first = False
    with os.fdopen(fd, 'r+b') as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        if stamp.exists():
            snapshot = json.loads(stamp.read_text())
            if (snapshot.get('format') != 'zil-private-request-tool-identities/1'
                    or snapshot.get('context') != context
                    or set(snapshot.get('files', {})) != paths
                    or snapshot.get('full_validate_completed') is not True):
                raise ValueError('identity stamp belongs to another/incomplete request')
            for record in snapshot['files'].values():
                digest = record.get('sha256', '')
                if len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest):
                    raise ValueError('invalid fixed image digest')
            _REQUEST_IDENTITIES = snapshot
            _guard_fixed_tools()
        else:
            _IDENTITY_CAPTURE_PATHS = paths
            _IDENTITY_CAPTURE = {'files': {}, 'full_hash_calls': 0, 'full_hash_bytes': 0}
            try:
                validate(p)
                snapshot = {'format': 'zil-private-request-tool-identities/1',
                            'context': context, 'full_validate_completed': True,
                            **_IDENTITY_CAPTURE}
                if set(snapshot['files']) != paths:
                    raise ValueError('complete validation did not inspect every fixed image')
                _REQUEST_IDENTITIES = snapshot
                _guard_fixed_tools()
                # Publish only after the entire old validate and read-before/
                # read-after stamps succeeded; followers hold the same flock.
                with stamp.open('x') as stream:
                    json.dump(snapshot, stream, indent=2)
                    stream.write('\n')
                first = True
            finally:
                _IDENTITY_CAPTURE = None
                _IDENTITY_CAPTURE_PATHS = set()
        fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
    if not first:
        # All original small protocol/source/build/CPP checks remain actual SHA.
        # Only fixed executable image reads use their freshly checked stamps.
        validate(p)
    _guard_fixed_tools()

def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')

def identity(path):
    # argv[0] selects the driver role for ld.lld/lld and rustc/rustup.
    # Keep the absolute invocation path; reading bytes follows symlinks for SHA.
    path = Path(os.path.abspath(path))
    path.resolve(strict=True)
    return {'path': str(path), 'sha256': sha(path)}

def scripts():
    return {str(p.relative_to(ROOT)): sha(p) for p in sorted(ROOT.rglob('*')) if p.is_file() and p.suffix in {'.py', '.cpp'}}

def validate(p):
    if p['format'] != 'zil-thinlto-private-linear-runtime/1' or p['status'] != 'prepared-not-run':
        raise ValueError('unknown protocol')
    if p['scripts'] != scripts():
        raise ValueError('script freeze changed')
    for item in p['tools'].values():
        if identity(item['path']) != item:
            raise ValueError('tool bytes/path changed')
    for arm, item in p['build_receipts'].items():
        if identity(item['path']) != item:
            raise ValueError('build binding receipt changed')
        binding = json.loads(Path(item['path']).read_text())
        compiler = p['tools']['baseline' if arm == 'A' else 'exporter']
        if (binding.get('source_commit') != p['source_claims'][arm]
                or binding.get('binary_sha256') != compiler['sha256']
                or str(Path(binding['binary']).resolve()) != compiler['path']
                or binding.get('release_build_status') != 'Pass'):
            raise ValueError('source and binary are not bound by successful build')
        expected_tree = '1c16744d838064cb1c687ba56ef3a0d17ed3130c' if arm == 'A' else 'ebb993128355fed8228627c6d7b37d5cb0a90d45'
        if binding.get('source_tree') != expected_tree or sha(binding['build_log']) != binding['build_log_sha256']:
            raise ValueError('source tree or build log binding changed')
    plugin_binding_path = p['plugin_build_receipt']
    if identity(plugin_binding_path['path']) != plugin_binding_path:
        raise ValueError('plugin build receipt changed')
    plugin_binding = json.loads(Path(plugin_binding_path['path']).read_text())
    if (plugin_binding['status'] != 'Pass'
            or plugin_binding['plugin'] != p['tools']['plugin']
            or identity(plugin_binding['source']['path']) != plugin_binding['source']
            or plugin_binding['source']['sha256'] != sha(ROOT / 'PrivateImports.cpp')
            or identity(plugin_binding['compiler']['path']) != plugin_binding['compiler']
            or plugin_binding['compiler']['sha256'] != p['tools']['clang']['sha256']
            or sha(plugin_binding['build_log']['path']) != plugin_binding['build_log']['sha256']):
        raise ValueError('plugin source/compiler/binary binding changed')
    if p['source_claims'] != {'A': MAIN, 'B': EXPORTER}:
        raise ValueError('source identity changed')
    if p['cases_requested'] != [64, 256] or p['iterations'] != 100000 or p['orders'] != ORDERS:
        raise ValueError('case or schedule changed')
    if p['limits'] != {'B_over_A_max': 1.10, 'R_over_B_min': .90}:
        raise ValueError('runtime gate changed')
    if p['mechanism'] != 'original-complete-budget-and-owner-proof; real-prelink-required; force-import-all-once; direct-only-private-postlink':
        raise ValueError('mechanism changed')
    if p['jobs'] != 2 or len(p['cpus']) != 2 or len(set(p['cpus'])) != 2:
        raise ValueError('two CPUs and two jobs required')
    if not set(p['cpus']).issubset(os.sched_getaffinity(0)):
        raise ValueError('frozen CPUs unavailable')

def run(command, log, timeout=600, data=None):
    _guard_fixed_tools()
    result = measured([str(v) for v in command], Path(log), timeout, data)
    write(Path(log) / 'raw-metrics.json', result)
    try:
        _guard_fixed_tools()
    except Exception as error:
        result['fixed_tool_after_guard'] = {'status': 'Failed', 'error': str(error)}
        write(Path(log) / 'raw-metrics.json', result)
        raise
    if result['exit'] != 0 or result['timeout'] or result['exception'] or result['residual_descendants']:
        raise RuntimeError(f'process failed, raw already saved: {log}')
    return result

def preflight(path, expected_units=8):
    x = json.loads(Path(path).read_text())
    if x.get('format') != 'zil-thinlto-export-preflight/1' or x.get('request') != 'O2+incremental-o2+after':
        raise ValueError('missing actual current exporter receipt')
    if x.get('backend') != 'clang' or x.get('imports') != 'disabled-for-llvm-thinlto':
        raise ValueError('wrong exporter policy')
    if x.get('actual_units') != expected_units or len(x.get('units', [])) != expected_units:
        raise ValueError('incomplete machine unit preflight')
    if x.get('original_preflight_ns', 0) <= 0 or x.get('partition_input_preflight_ns', 0) <= 0:
        raise ValueError('full original/partition model preflight absent')
    if [v['unit'] for v in x['units']] != list(range(expected_units)) or any(v['preflight_ns'] <= 0 for v in x['units']):
        raise ValueError('unit preflight absent')
    from visibility import export_proof
    export_proof(x)
    return x
