#!/usr/bin/env python3
"""Read and hash frozen build evidence only; launch no compiler or harness."""
import datetime
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path('/workspace/scratch/c3607737969d')
OUT = ROOT / 'review'
COMMITS = {'cp32': '3ba131efdeffe85ba5fccd1c177745d812a6ce61',
           'cp33': '83854ae83f7fec4fc8f9c22d0751a1fd2725671f'}
DIRS = {'cp32': ROOT / 'recovery/cp32-release-3ba131ef',
        'cp33': ROOT / 'recovery/cp33-release-83854ae83'}
SOURCE_SHA = '3aaf249ad90b67029bacedc66523ce149c230099af27fdc22ed2a6074ee5a37f'
EXPECTED_FEATURES = {'zil_driver': ['default', 'llvm-api'],
                     'zil_backend_llvm': ['default', 'llvm-api'],
                     'zil_llvm_api': ['default', 'native']}
HASH_CACHE = {}
verified = {}

def sha(path):
    p = Path(path); st = p.stat()
    key = (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns, st.st_ctime_ns)
    if key not in HASH_CACHE:
        with p.open('rb') as f: HASH_CACHE[key] = hashlib.file_digest(f, 'sha256').hexdigest()
    return HASH_CACHE[key]

def read(p): return json.loads(Path(p).read_text())

def item(record):
    p = Path(record['path'])
    assert p.stat().st_size == record['size'] and sha(p) == record['sha256'], p
    assert str(p.resolve()) == record['resolved'], p
    verified[str(p)] = {'size': p.stat().st_size, 'sha256': sha(p)}

def fingerprints(target):
    result = {}
    for crate, features in EXPECTED_FEATURES.items():
        paths = list((target / 'release/.fingerprint').glob(crate.replace('_', '-') + '-*/lib-' + crate + '.json'))
        assert len(paths) == 1, (crate, paths)
        p = paths[0]; d = read(p)
        assert json.loads(d['features']) == features, (p, d['features'])
        assert d['rustflags'] == []
        result[crate] = {k: d[k] for k in ('rustc', 'profile', 'compile_kind', 'features', 'rustflags')}
        verified[str(p)] = {'size': p.stat().st_size, 'sha256': sha(p)}
    return result

baseline = fingerprints(ROOT / 'build-baseline')
records = {}
for name, commit in COMMITS.items():
    directory = DIRS[name]; receipt_path = directory / 'BUILD-RECEIPT.json'
    receipt = read(receipt_path); repo = ROOT / ('zil-' + name)
    assert receipt['source_commit'] == commit
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip() == commit
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo, text=True).strip()
    source = directory / 'compile7_session.rs'
    assert sha(source) == SOURCE_SHA
    features = {}; profiles = {}
    cargo_stdout = directory / 'cargo-build.stdout'
    for line in cargo_stdout.read_text().splitlines():
        message = json.loads(line)
        if message.get('reason') == 'compiler-artifact':
            crate = message['target']['name']
            features[crate] = message['features']; profiles[crate] = message['profile']
    for crate, values in EXPECTED_FEATURES.items():
        assert features[crate] == values
        assert profiles[crate] == {'opt_level': '3', 'debuginfo': 0, 'debug_assertions': False,
                                   'overflow_checks': False, 'test': False}
    assert fingerprints(ROOT / ('target-' + name)) == baseline
    if name == 'cp32':
        request = read(directory / 'build-request.json')
        assert request['source_commit'] == commit and request['source_status'] == receipt['source_status'] == ''
        assert receipt['cargo']['returncode'] == 0
        for record in receipt['artifacts'] + receipt['tools']: item(record)
        links = receipt['link_commands']; assert all(x['returncode'] == 0 for x in links)
        command = links[0]['command']
        assert command[command.index('--cfg') + 1] == 'cp29' and command.count('--cfg') == 1
        for stem in ['session-harness-link', 'native-control-link']:
            for stream in ['stdout', 'stderr']:
                p = directory / f'{stem}.{stream}'; assert p.read_bytes() == b''
                verified[str(p)] = {'size': 0, 'sha256': sha(p)}
        binary = ROOT / 'recovery/compile7_tools/session-cp32'
        assert str(source) in command and str(binary) == command[-1]
        linked_rlibs = [x.split('=', 1)[1] for x in command if x.startswith(('zil_driver=', 'zil_base='))]
        for p in linked_rlibs: assert p in verified
        native_control_source = directory / 'cp29_native_control.rs'
        assert sha(native_control_source) == '5275c621600ba7a50a52b351a26f1f0a0a12a6a26deff6051f5c2c3abbda037f'
    else:
        assert receipt['source_clean_before'] and receipt['source_clean_after']
        assert receipt['source_commit_after'] == commit and receipt['source_status_after'] == ''
        assert receipt['source_crates_tree'] == subprocess.check_output(['git', 'rev-parse', 'HEAD:crates'], cwd=repo, text=True).strip()
        assert receipt['cargo']['returncode'] == receipt['link']['returncode'] == 0
        assert receipt['features'] == features and receipt['profiles'] == profiles
        for record in receipt['rlibs'] + receipt['tools'] + [receipt['harness_source'], receipt['binary'],
                receipt['cargo']['stdout'], receipt['cargo']['stderr'], receipt['link']['stdout'], receipt['link']['stderr']]: item(record)
        assert Path(receipt['link']['stdout']['path']).read_bytes() == b''
        assert Path(receipt['link']['stderr']['path']).read_bytes() == b''
        command = receipt['link']['command']; assert '--cfg' not in command
        binary = Path(receipt['binary']['path'])
        assert str(source) in command and str(binary) == command[-1]
        linked_rlibs = [x.split('=', 1)[1] for x in command if x.startswith(('zil_driver=', 'zil_base='))]
        for p in linked_rlibs: assert p in verified
        deps = ROOT / 'target-cp33/release/deps/libzil_driver-8b7b7327dd24ab30.rlib'
        assert deps.samefile(ROOT / 'target-cp33/release/libzil_driver.rlib')
    assert command[:8] == ['rustc', '--edition=2024', '-O', '-C', 'debuginfo=0', '-C', 'panic=abort', '-L']
    assert 'native=/workspace/scratch/4ab361722127/.toolchain/llvm/lib' in command
    records[name] = {'source_commit': commit, 'receipt_path': str(receipt_path), 'receipt_sha256': sha(receipt_path),
                     'same_harness_source_sha256': sha(source), 'binary': str(binary), 'binary_sha256': sha(binary),
                     'command': command, 'features': features, 'profiles': profiles,
                     'cfg_cp29': name == 'cp32', 'source_currently_clean': True}

for path in [ROOT / 'recovery/compile7_tools/compile7_session.rs',
             ROOT / 'zil/tools/research/compile-performance/compile7_session.rs']:
    assert sha(path) == SOURCE_SHA
    verified[str(path)] = {'size': path.stat().st_size, 'sha256': sha(path)}

result = {'reviewer': 'cp25_evidence', 'verdict': 'Pass',
          'at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'scope': 'Read-only build identity, metadata and artifact digest review. No compiler/harness/native execution.',
          'candidates': records, 'baseline_matching_features_profiles': baseline,
          'verified_files': verified, 'verified_file_count': len(verified),
          'harness_cfg_boundary': 'Only CP32 uses cfg cp29 for memo statistics after compile_path timing. Both use the unchanged frozen source. Whole-process metrics include reporting.',
          'audit_script_sha256': sha(__file__)}
(OUT / 'CP32-CP33-STATIC-BUILD-REVIEW.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({'verdict': 'Pass', 'files': len(verified),
                  'binaries': {k: v['binary_sha256'] for k, v in records.items()}}, indent=2))
