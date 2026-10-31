#!/usr/bin/env python3
"""Read retained artifacts only. Never launch a compiler or native executable."""
from pathlib import Path
import hashlib
import json
import struct

ROOT = Path('/workspace/scratch/c3607737969d')
REVIEW = ROOT / 'review'
BUILD = ROOT / 'recovery/cp32-release-3ba131ef'
DATA = BUILD / 'native-control'


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


receipt = json.loads((BUILD / 'NATIVE-CONTROL-RECEIPT.json').read_text())
assert sha(BUILD / 'cp29_native_control.rs') == receipt['control_source_sha256']
assert sha(BUILD / 'cp32-native-control') == receipt['control_binary_sha256']
assert receipt['source_commit'] == '3ba131efdeffe85ba5fccd1c177745d812a6ce61'
for name, digest in receipt['files'].items():
    assert sha(DATA / name) == digest, name

records = []
for step, value in [('cold', 1), ('private', 5), ('restore', 1)]:
    entry = {'step': step, 'source_leaf_value': value, 'expected_return': value + 2,
             'expected_stdout_hex': bytes([64 + value + 2]).hex(), 'arms': {}}
    for arm in ['warm', 'fresh']:
        directory = DATA / step / arm
        report = json.loads((directory / 'compile.json').read_text())
        exits = json.loads((directory / 'exit.json').read_text())
        assert report['status'] == 'Success' and report['verification_runs'] == 1
        assert report['phase'] == 'link'
        assert report['native_codegen']['backend'] == 'clang'
        assert exits == {'actual': value + 2, 'expected': value + 2}
        assert (directory / 'stdout').read_bytes() == bytes([64 + value + 2])
        assert (directory / 'stderr').read_bytes() == b''
        elf = (directory / 'program').read_bytes()
        assert elf[:4] == b'\x7fELF' and elf[4] == 2 and elf[5] == 1
        assert struct.unpack_from('<H', elf, 18)[0] == 62
        commands = report['commands']
        assert len(commands) == 1 and commands[0][0] == 'clang'
        assert '-x' in commands[0] and commands[0][commands[0].index('-x') + 1] == 'ir'
        assert '-O0' in commands[0] and '--target=x86_64-unknown-linux-gnu' in commands[0]
        entry['arms'][arm] = {
            'status': report['status'], 'phase': report['phase'], 'verification_runs': 1,
            'exit': exits, 'stdout_hex': (directory / 'stdout').read_bytes().hex(),
            'executable_sha256': sha(directory / 'program'), 'executable_size': len(elf),
            'machine': 'ELF64 little-endian x86_64', 'native_backend': 'clang',
            'commands': commands,
        }
    warm = json.loads((DATA / step / 'warm/compile.json').read_text())
    fresh = json.loads((DATA / step / 'fresh/compile.json').read_text())
    for key in ['diagnostics', 'scalar_interfaces', 'input_fingerprint', 'source_files']:
        assert warm[key] == fresh[key], (step, key)
    entry['representation_sha256'] = {}
    for phase, filename in [('typed', 'typed.txt'), ('candidate', 'candidate.txt'),
                             ('lir', 'lir.txt'), ('llvm', 'llvm.ll'),
                             ('verified-interfaces', 'verified-interfaces.txt')]:
        a = DATA / step / 'warm/phases' / filename
        b = DATA / step / 'fresh/phases' / filename
        assert a.read_bytes() == b.read_bytes(), (step, phase)
        entry['representation_sha256'][phase] = sha(a)
    assert entry['arms']['warm']['executable_sha256'] == entry['arms']['fresh']['executable_sha256']
    memo = json.loads((DATA / step / 'warm/memo.json').read_text())
    assert memo['eligible'] is True and memo['retained_modules'] == 3
    assert memo['reused_functions'] == (0 if step == 'cold' else 2)
    assert memo['checked_functions'] == (6 if step == 'cold' else 4)
    entry['memo'] = memo
    records.append(entry)

warm = json.loads((DATA / 'reject/warm.json').read_text())
fresh = json.loads((DATA / 'reject/fresh.json').read_text())
assert warm['status'] == fresh['status'] == 'SafeReject'
assert warm['phase'] == fresh['phase'] == 'types'
assert warm['diagnostics'] == fresh['diagnostics']
assert warm['verification_runs'] == fresh['verification_runs'] == 0
assert warm['artifacts'] == fresh['artifacts'] == {}
assert warm['native_codegen'] is fresh['native_codegen'] is None
assert not (DATA / 'reject/must-not-publish').exists()
assert (BUILD / 'native-control.stderr').read_bytes() == b''

summary = {
    'reviewer': 'cp25_evidence', 'verdict': 'Pass',
    'scope': 'Read-only artifact/source audit of author-executed native control; reviewer launched no compiler/native binary',
    'author_source_commit': receipt['source_commit'],
    'control_source_sha256': sha(BUILD / 'cp29_native_control.rs'),
    'control_binary_sha256': sha(BUILD / 'cp32-native-control'),
    'receipt_sha256': sha(BUILD / 'NATIVE-CONTROL-RECEIPT.json'),
    'manifest_files_verified': len(receipt['files']),
    'manifest_total_bytes': sum((DATA / name).stat().st_size for name in receipt['files']),
    'native_executions_in_author_evidence': 6,
    'independently_rerun_native_executions': 0,
    'backend': 'clang textual LLVM IR compilation and linking at O0, not LLVM API native backend',
    'source_oracle': 'main computes n=a.get()+b.get(), a returns 1/5/1, b returns 2; io.put(64+n) calls putchar. Expected exits 3/7/3 and stdout C/G/C follow directly from this source.',
    'requests': records,
    'rejected_pair': {'diagnostics': warm['diagnostics'], 'status': 'SafeReject', 'phase': 'types',
                      'verification_runs': 0, 'artifacts_empty': True, 'no_output_published': True},
    'retained_epoch_evidence': 'The frozen control source asserts retained_type_memo_stats unchanged after the rejected edit. Following restore records reused=2/checked=4 and matches fresh IR/native outputs. This review does not rerun the assertion.',
    'limitations': ['Only this finite scalar/FFI private-edit/restore path at Linux x86_64 O0',
                    'One intermediate signature error; not all native-build failure modes',
                    'This small 3-retained-module native control does not cross the CP32 partial-retention capacity boundary',
                    'No new native execution or performance measurement by reviewer'],
    'audit_script_sha256': sha(__file__),
}
output = REVIEW / 'CP32-NATIVE-STATIC-ARTIFACTS.json'
output.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({key: summary[key] for key in
                  ['verdict', 'manifest_files_verified', 'manifest_total_bytes',
                   'native_executions_in_author_evidence', 'independently_rerun_native_executions', 'backend']}, indent=2))
