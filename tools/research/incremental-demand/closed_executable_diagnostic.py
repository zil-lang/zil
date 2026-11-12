#!/usr/bin/env python3
"""Plan an additional same-policy trial, or inspect existing raw CGU products.
Never invokes a compiler, worker, ELF, or measurement command. Standard library only.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shlex
import struct

from push_counters import llvm_comparison_sha256


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def controls(manifest, closed):
    a = manifest['arguments']
    for key, expected in [('case', 'scalar'), ('modules', 20), ('functions', 20),
                          ('steps', 16), ('mode', 'O2'), ('backend', 'llvm-api'),
                          ('codegen_units', 16), ('codegen_jobs', 1)]:
        require(a[key] == expected, f'changed fixed-fixture control: {key}')
    all_flags = a['extra_arg'] + a['baseline_extra_arg'] + a['candidate_extra_arg']
    require(set(all_flags) <= {'--profile-compilation', '--push-reuse', '--closed-executable'},
            'unreviewed policy/budget/fixture flags')
    if closed:
        common_closed = a.get('closed_executable', False) or '--closed-executable' in a['extra_arg']
        require(common_closed, 'closed policy must be explicitly common to both arms')
    return a


def plan(args):
    manifest = json.loads((args.reference_run / 'manifest.json').read_text())
    a = controls(manifest, False)
    require(not args.out_run.exists(), 'new run directory must not exist')
    require(args.blocks >= 1, 'at least one diagnostic block is required')
    require(re.fullmatch('[0-9a-f]{40}', args.candidate_source), 'candidate must be an exact frozen source commit')
    runner = args.repo / 'tools/research/reactive-query-graph/native_measure.py'
    cmd = ['python3', str(runner), '--baseline', a['baseline'], '--candidate', str(args.candidate),
           '--baseline-source', a['baseline_source'], '--candidate-source', args.candidate_source,
           '--baseline-llvm-library-path', a['baseline_llvm_library_path'],
           '--candidate-llvm-library-path', str(args.candidate_llvm_library_path),
           '--out', str(args.out_run), '--case', 'scalar', '--modules', '20', '--functions', '20',
           '--steps', '16', '--mode', 'O2', '--backend', 'llvm-api', '--codegen-units', '16',
           '--codegen-jobs', '1', '--blocks', str(args.blocks), '--cache', a['cache'],
           '--capacity', str(a['capacity']), '--timeout', str(a['timeout']),
           '--native-timeout', str(a['native_timeout']), '--sample-ms', str(a['sample_ms'])]
    if a.get('source_cache'):
        cmd += ['--source-cache']
    for option, values in [('--extra-arg', a['extra_arg'] + ['--closed-executable']),
                           ('--baseline-extra-arg', a['baseline_extra_arg']),
                           ('--candidate-extra-arg', a['candidate_extra_arg'])]:
        for value in dict.fromkeys(values):
            cmd.append(option + '=' + value)
    for request in manifest['requests']:
        cmd += ['--request', request]
    return {'status': 'planned-not-executed', 'argv': cmd, 'shell': shlex.join(cmd),
            'reference_manifest_sha256': sha((args.reference_run / 'manifest.json').read_bytes()),
            'runner_sha256': sha(runner.read_bytes()),
            'policy': 'Both ZIL arms explicitly use existing closed-executable; defaults and old results unchanged.',
            'required_post_run_oracles': ['all successful semantic/native checks', 'paired full ELF identity',
                                         'paired full LLVM text identity', 'current independent Verify once',
                                         'per-unit pre-optimization members and actual emitted ELF functions'],
            'limitation': 'Additional policy diagnostic, not a default-policy replacement or a Rust comparison.'}


def module_items(path):
    data = path.read_bytes()
    items = []
    for line in data.decode().splitlines():
        if line.startswith(('define ', 'declare ')):
            match = re.search(r'@([A-Za-z0-9_.-]+)\(', line)
            require(match is not None, 'unsupported function spelling in LLVM item parser')
            items.append({'name': match[1], 'kind': 'function', 'definition': line.startswith('define ')})
        elif line.startswith(('@', '%')):
            # This specific scalar fixture has no machine globals/named types;
            # decline rather than guessing their full model item encoding.
            raise ValueError('non-function item outside fixed scalar diagnostic parser')
    require(len({i['name'] for i in items}) == len(items), 'duplicate LLVM item names')
    return items, sha(data)


def layout(path, items):
    raw = path.read_bytes()
    require(len(raw) <= 8*1024*1024+72 and raw[:8] == b'ZILCGU01', 'invalid layout envelope')
    require(raw[8:40].hex() == path.stem and sha(raw[72:]) == raw[40:72].hex(), 'layout checksum/key mismatch')
    pos = 72
    def word():
        nonlocal pos
        require(pos + 4 <= len(raw), 'truncated layout')
        n, = struct.unpack_from('<I', raw, pos)
        pos += 4
        return n
    count = word()
    require(count == 16, 'actual layout must have 16 units')
    units = []
    for index in range(count):
        unit = {'unit_index': index, 'bucket': word()}
        for name in ['prelude', 'declarations', 'imports', 'definitions']:
            size = word()
            require(size <= 1_048_576, 'layout ref bound')
            indices = [word() for _ in range(size)]
            require(all(i < len(items) for i in indices), 'layout index outside actual module')
            unit[name] = [items[i]['name'] for i in indices]
        require(not unit['imports'], 'incremental-O2 import policy must remain disabled')
        units.append(unit)
    require(pos == len(raw), 'trailing layout bytes')
    owners = [n for u in units for n in u['definitions']]
    expected = [i['name'] for i in items if i['definition']]
    require(len(owners) == len(set(owners)) and set(owners) == set(expected), 'layout definition coverage')
    return units, sha(raw)


def elf_functions(data):
    require(data[:7] == b'\x7fELF\x02\x01\x01', 'ELF64 little endian required')
    h = struct.unpack_from('<16sHHIQQQIHHHHHH', data)
    require(h[1:3] == (1, 62) and h[11] == 64 and 0 < h[12] < 65536, 'unsupported object header')
    def take(off, length):
        require(0 <= off <= len(data) and 0 <= length <= len(data)-off, 'ELF section bounds')
        return data[off:off+length]
    sections = [struct.unpack('<IIQQQQIIQQ', take(h[6]+64*i, 64)) for i in range(h[12])]
    found = []
    for section in sections:
        if section[1] != 2:
            continue
        require(section[9] == 24 and section[5] % 24 == 0 and section[6] < len(sections), 'ELF symbol table')
        strings = sections[section[6]]
        strings = take(strings[4], strings[5])
        rows = take(section[4], section[5])
        for off in range(0, len(rows), 24):
            name, info, other, sec, value, size = struct.unpack_from('<IBBHQQ', rows, off)
            if info & 15 != 2 or not 0 < sec < len(sections):
                continue
            require(name < len(strings), 'symbol name bounds')
            end = strings.find(b'\0', name)
            require(end >= name, 'unterminated symbol name')
            found.append({'name': strings[name:end].decode(), 'bytes': size,
                          'binding': {0:'LOCAL', 1:'GLOBAL', 2:'WEAK'}.get(info >> 4, str(info >> 4)),
                          'visibility': other, 'nonzero_body': size != 0})
    return found


def cached_elf(worker, key):
    require(re.fullmatch('[0-9a-f]{64}', key) is not None, 'bad object action key')
    paths = list((worker / 'objects/objects-v1').glob('*/' + key + '.zobj'))
    require(len(paths) == 1, f'expected unique current object for {key}')
    p = paths[0]
    require(p.resolve().is_relative_to(worker.resolve()), 'cached object outside worker')
    require(p.is_file() and not p.is_symlink() and 80 <= p.stat().st_size <= 128*1024*1024+80, 'invalid zobj file')
    raw = p.read_bytes()
    length, = struct.unpack_from('<Q', raw, 72)
    require(raw[:8] == b'ZILOBJ01' and raw[8:40].hex() == key and len(raw) == length+80, 'zobj binding/length')
    data = raw[80:]
    require(sha(data) == raw[40:72].hex(), 'zobj full payload hash')
    return data, str(p), sha(raw)


def inspect(args):
    root = args.run_dir.resolve()
    manifest_data = (root / 'manifest.json').read_bytes()
    manifest = json.loads(manifest_data)
    controls(manifest, not args.allow_default)
    rows_data = (root / 'rows.jsonl').read_bytes()
    rows = [json.loads(line) for line in rows_data.splitlines()]
    require(rows and all(r['valid'] and r['paired_semantics_equal'] for r in rows), 'invalid semantic row')
    pairs = {}
    for row in rows:
        pairs.setdefault((row['block'], row['request_index'], row['request'], row['measured']), []).append(row)
    raw_llvm_identity_failures = []
    normalized_llvm_identity_failures = []
    checked_llvm = {}
    for pair_id, pair in pairs.items():
        require(len(pair) == 2 and {r['arm'] for r in pair} == {'baseline','candidate'}, 'incomplete pair')
        if pair[0]['status'] == 'Success':
            require(all(r['resident']['report']['verification_runs'] == 1 and r['native'] and
                        all(n['valid'] for n in r['native']) for r in pair), 'current Verify/native outcomes')
            require(pair[0]['executable_sha256'] == pair[1]['executable_sha256'], 'failed full ELF identity')
            if pair[0]['llvm_text_sha256'] != pair[1]['llvm_text_sha256']:
                raw_llvm_identity_failures.append({'pair': list(pair_id), 'sha256': {r['arm']: r['llvm_text_sha256'] for r in pair}})
            comparisons = []
            for row in pair:
                archive = row.get('llvm_archive')
                if not archive:
                    comparisons.append(None)
                    continue
                path = (root / archive['path']).resolve()
                require(path.is_relative_to(root) and path.is_file(), 'LLVM archive outside run')
                if path not in checked_llvm:
                    data = path.read_bytes()
                    require(sha(data) == row['llvm_text_sha256'] == archive['sha256'] and len(data) == archive['bytes'], 'raw LLVM archive binding')
                    checked_llvm[path] = llvm_comparison_sha256(data)
                require(checked_llvm[path] == row['llvm_comparison_sha256_excluding_identity_comment'], 'normalized LLVM digest differs from actual full bytes')
                comparisons.append(checked_llvm[path])
            if None in comparisons or comparisons[0] != comparisons[1]:
                normalized_llvm_identity_failures.append({'pair': list(pair_id), 'comparison_sha256': comparisons})
    inventories = []
    for arm in ['baseline', 'candidate']:
        cold = next(r for r in rows if r['block'] == 0 and r['request'] == 'cold' and r['measured'] and r['arm'] == arm)
        worker = root / f'block-0-{arm}'
        items, llvm_sha = module_items(worker / 'program.ll')
        retained_comparison = llvm_comparison_sha256((worker / 'program.ll').read_bytes())
        require(llvm_sha == cold['llvm_text_sha256'], 'retained LLVM must match the cold row (normally restored cold); archive it otherwise')
        layouts = list((worker / 'objects/partition-layout-v1').glob('*.layout'))
        layout_records = []
        units = None
        for path in layouts:
            decoded, digest = layout(path, items)
            require(units is None or units == decoded, 'historical layouts disagree on actual membership')
            units = decoded
            layout_records.append({'path': str(path), 'sha256': digest})
        if units is None:
            units = [{'unit_index': i, 'preoptimization_members': 'not recorded by this baseline'} for i in range(16)]
        reports = cold['resident']['report']['native_codegen']['object_reports']
        require(len(reports) == 16, 'actual report units changed')
        for unit, report in zip(units, reports):
            key = report['cache']['action_key']
            data, path, cache_sha = cached_elf(worker, key)
            emitted = elf_functions(data)
            unit.update({'action_key': key, 'object_path': path, 'cache_file_sha256': cache_sha,
                         'elf_sha256': sha(data), 'emitted_stt_func': emitted,
                         'emitted_nonzero_function_bodies': sum(x['nonzero_body'] for x in emitted),
                         'defined_ir_names_without_emitted_symbol': sorted(set(unit['definitions']) - {x['name'] for x in emitted}) if 'definitions' in unit else None})
        inventories.append({'arm': arm, 'llvm_sha256': llvm_sha, 'llvm_comparison_sha256_excluding_identity_comment': retained_comparison, 'equivalent_layout_records': layout_records, 'units': units})
    return {'status': 'read-only-existing-products-inspected', 'manifest_sha256': sha(manifest_data),
            'rows_sha256': sha(rows_data), 'all_rows': len(rows),
            'qualification_passed': not normalized_llvm_identity_failures and (not raw_llvm_identity_failures or '--push-reuse' in manifest['arguments']['candidate_extra_arg']),
            'normalized_llvm_identity_failures': normalized_llvm_identity_failures,
            'raw_llvm_identity_failures': raw_llvm_identity_failures, 'successful_pairs': sum(p[0]['status']=='Success' for p in pairs.values()),
            'policy': 'original-default-control' if args.allow_default else 'additional-common-closed-executable-policy',
            'inventories': inventories,
            'retained_cold_llvm_equal_excluding_identity_comment': len({a['llvm_comparison_sha256_excluding_identity_comment'] for a in inventories}) == 1,
            'scope': 'Layout indices bound to retained cold raw LLVM item order plus checksummed existing cache payloads. No new compiler/native execution.',
            'limits': 'STT_FUNC bodies are emitted symbols, not source-function counts or a safe LLVM-skipping proof. No timing conclusion; pre-CGU layout checksums are raw integrity, not Verify authority.'}


def inspect_rust(args):
    root = args.run_dir.resolve()
    request = (root / args.request_dir / 'request.json').resolve()
    require(request.is_relative_to(root), 'request outside run')
    raw = request.read_bytes()
    row = json.loads(raw)
    require(row['arm'] == 'rust' and row['valid'] and row['mode'] == 'o2', 'expected valid archived Rust O2 request')
    units = []
    for obj in row['inventory']['objects']:
        path = (root / obj['object']['archive']).resolve()
        require(path.is_relative_to(root) and path.is_file() and path.stat().st_size <= 128*1024*1024, 'Rust object archive bound')
        data = path.read_bytes()
        require(sha(data) == obj['object']['sha256'] and len(data) == obj['object']['bytes'], 'Rust object full hash/length')
        units.append({'object_name': obj['name'], 'archive': obj['object']['archive'],
                      'sha256': sha(data), 'bytes': len(data),
                      'same_bytes_as_previous_request': obj['same_bytes_as_previous_request'],
                      'emitted_stt_func': elf_functions(data)})
    return {'status': 'read-only-existing-Rust-products-inspected', 'request_path': str(request),
            'request_sha256': sha(raw), 'units': units, 'actual_objects': len(units),
            'actual_defined_stt_func': sum(len(u['emitted_stt_func']) for u in units),
            'changed_objects': [u for u in units if not u['same_bytes_as_previous_request']],
            'scope': 'Existing raw object/hash inventory, not new Rust execution or measurement. Emitted symbol counts are not source-function or equal-partition claims.'}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest='mode', required=True)
    q = sub.add_parser('plan')
    q.add_argument('--repo', type=Path, required=True)
    q.add_argument('--reference-run', type=Path, required=True)
    q.add_argument('--out-run', type=Path, required=True)
    q.add_argument('--candidate', type=Path, required=True)
    q.add_argument('--candidate-source', required=True)
    q.add_argument('--candidate-llvm-library-path', type=Path, required=True)
    q.add_argument('--blocks', type=int, default=1)
    q = sub.add_parser('inspect')
    q.add_argument('--run-dir', type=Path, required=True)
    q.add_argument('--allow-default', action='store_true', help='inspect retained default control, not a new closed trial')
    q = sub.add_parser('rust-inventory')
    q.add_argument('--run-dir', type=Path, required=True)
    q.add_argument('--request-dir', type=Path, required=True, help='relative directory of an archived formal Rust request')
    p.add_argument('--output', type=Path)
    a = p.parse_args()
    result = {'plan': plan, 'inspect': inspect, 'rust-inventory': inspect_rust}[a.mode](a)
    text = json.dumps(result, indent=2) + '\n'
    if a.output:
        a.output.write_text(text)
    else:
        print(text, end='')
    if result.get('qualification_passed') is False:
        raise SystemExit(1)

if __name__ == '__main__':
    main()
