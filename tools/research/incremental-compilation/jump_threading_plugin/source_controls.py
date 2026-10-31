#!/usr/bin/env python3
"""Actual ZIL ownership/FFI/function-pointer and transactional-cache controls."""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import zipfile
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source():
    text = ('extern "C" fn putchar(n:i32)->i32;\n'
            'resource Token{code:i32}\n'
            'impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}\n'
            'fn make(n:i32)->Token contract{result(create)}{return Token{code:n}}\n'
            'fn early()->i32{let t=make(65);let f=move()=>t.code;return f()-65;}\n')
    text += ''.join(f'fn padding{i}(x:i32)->i32{{return x+1;}}\n' for i in range(600))
    text += ('fn late()->i32{let t=make(66);let f=move()=>t.code;'
             'let p:fn(i32)->i32=padding0;return f()+p(0)-67;}\n'
             'fn main()->i32{return early()+late();}\n')
    return text


def inventory(path):
    return {str(p.relative_to(path)): sha(p) for p in sorted(path.rglob('*')) if p.is_file()}


def read_bundle(path):
    with zipfile.ZipFile(path) as bundle:
        names = bundle.namelist()
        assert len(names) == 3 and set(names) == {'receipt.json', 'object.o', 'optimized.bc'}
        assert all(item.compress_type == zipfile.ZIP_STORED for item in bundle.infolist())
        # Reading each member also checks its CRC, independently of the stored
        # content hashes checked below against the actual unit input receipt.
        return {name: bundle.read(name) for name in names}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['zil', 'clang', 'opt', 'plugin', 'lld', 'pipeline-manifest',
                 'snapshot-receipt', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    (root / 'sources').mkdir()
    (root / 'work').mkdir()
    working = root / 'work/main.zil'
    original = source()
    working.write_text(original)
    here = Path(__file__).resolve().parent
    runner = here / 'run_request.py'
    wrapper = here / 'clang_wrapper.py'
    frozen = {str(p): sha(p) for p in [runner, wrapper, args.pipeline_manifest,
                                      args.snapshot_receipt, args.plugin]}
    receipt = {'scope': __doc__, 'expected_native_stdout_hex': '4142',
               'frozen_inputs': frozen, 'rows': [], 'valid': False,
               'timing_scope': 'functional control window; no performance conclusions'}

    def save():
        (root / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')

    def check_frozen():
        assert all(sha(Path(p)) == digest for p, digest in frozen.items()), 'tool inputs changed'

    def build(label, arm, closed, cache):
        check_frozen()
        snapshot = root / 'sources' / (label + '.zil')
        snapshot.write_bytes(working.read_bytes())
        output = root / label
        command = [sys.executable, str(runner)]
        for name in ['zil', 'clang', 'opt', 'plugin', 'lld', 'pipeline-manifest', 'snapshot-receipt']:
            # Preserve multicall tool basenames, particularly ld.lld -> lld.
            command += ['--' + name, str(getattr(args, name.replace('-', '_')).absolute())]
        command += ['--source', str(working), '--cache', str(cache), '--arm', arm,
                    '--out', str(output), '--verify-analyses', '--incremental-o2',
                    '--expected-hex', '4142', '--units', '8', '--jobs', '1']
        if closed:
            command += ['--closed']
        with (root / (label + '.runner.stdout')).open('wb') as so, \
                (root / (label + '.runner.stderr')).open('wb') as se:
            run = subprocess.run(command, stdout=so, stderr=se)
        row = {'label': label, 'arm': arm, 'closed': closed, 'command': command,
               'source_snapshot': str(snapshot), 'source_sha256': sha(snapshot),
               'runner_returncode': run.returncode, 'output': str(output)}
        receipt['rows'].append(row)
        if (output / 'result.json').exists():
            row['result'] = json.loads((output / 'result.json').read_text())
        save()
        assert run.returncode == 0 and row['result']['valid'], label
        result = row['result']
        assert result['verification_runs'] == 1
        assert result['native']['exit'] == 0 and result['native']['stdout_hex'] == '4142'
        inputs = [json.loads(p.read_text()) for p in sorted(output.glob('unit-*.ll.input.json'))]
        assert len(inputs) == result['current_preflight_units'] > 1
        assert len(inputs) == result['native_codegen']['actual_units']
        row['keys'] = []
        for unit in inputs:
            material = unit['material']
            key = hashlib.sha256(json.dumps(material, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
            assert key == unit['action_key'] and material['arm'] == arm and material['schema'] == 2
            assert unit['current_machine_preflight'] is True
            assert unit['object_sha256'] == sha(output / f"unit-{unit['unit']}.ll.o")
            assert unit['bitcode_sha256'] == sha(output / f"unit-{unit['unit']}.ll.optimized.bc")
            # Use the actual current request's bundle path, never a guessed or
            # arbitrary historical object. Keep the destructive control local.
            bundle_path = Path(unit['cache_entry_path'])
            assert bundle_path.resolve().parent == cache.resolve()
            assert bundle_path.name == key + '.zip'
            members = read_bundle(bundle_path)
            cached = json.loads(members['receipt.json'])
            assert cached['material'] == material
            assert cached['object_sha256'] == unit['object_sha256'] == hashlib.sha256(members['object.o']).hexdigest()
            assert cached['bitcode_sha256'] == unit['bitcode_sha256'] == hashlib.sha256(members['optimized.bc']).hexdigest()
            row['keys'].append({'unit': unit['unit'], 'action_key': key,
                                'cache_entry_path': str(bundle_path),
                                'cache_hit': unit['cache_hit'], 'corrupt_rebuilt': unit['corrupt_rebuilt'],
                                'raw_sha256': unit['raw_sha256'], 'normalized_sha256': unit['normalized_sha256'],
                                'object_sha256': unit['object_sha256'], 'bitcode_sha256': unit['bitcode_sha256']})
        row['valid'] = True
        save()
        print(json.dumps({'label': label, 'valid': True, 'units': len(inputs),
                          'hits': result['cache_hits'], 'misses': result['cache_misses'],
                          'corrupt_rebuilt': result['corrupt_rebuilt']}), flush=True)
        return row

    try:
        cold = {}
        for arm in ['original', 'entry']:
            for closed in [False, True]:
                label = arm + ('-closed' if closed else '-default')
                cache = root / 'cache' / label
                row = build(label + '-cold', arm, closed, cache)
                assert row['result']['cache_hits'] == 0
                cold[(arm, closed)] = row
        cache = root / 'cache/entry-default'
        baseline = cold[('entry', False)]
        warm = build('entry-default-warm', 'entry', False, cache)
        assert warm['result']['cache_hits'] == warm['result']['current_preflight_units']
        assert warm['result']['cache_misses'] == 0
        assert [k['action_key'] for k in warm['keys']] == [k['action_key'] for k in baseline['keys']]

        current = warm['keys'][0]
        victim = Path(current['cache_entry_path'])
        assert victim.resolve().parent == cache.resolve()
        assert victim.name == current['action_key'] + '.zip'
        members = read_bundle(victim)
        original_receipt = members['receipt.json']
        receipt['object_corruption'] = {'path': str(victim), 'actual_current_key': current['action_key'],
                                        'component': 'object.o', 'unit': current['unit'],
                                        'before_sha256': hashlib.sha256(members['object.o']).hexdigest(),
                                        'bundle_before_sha256': sha(victim)}
        members['object.o'] = b'ZIL-JT-CONTROL-CORRUPT-OBJECT\n'
        # Produce a structurally valid ZIP with fresh CRCs but stale declared
        # object hash. This exercises content validation, not just ZIP parsing.
        with zipfile.ZipFile(victim, 'w', compression=zipfile.ZIP_STORED) as bundle:
            for name, contents in members.items():
                bundle.writestr(name, contents)
        changed_members = read_bundle(victim)
        assert changed_members['receipt.json'] == original_receipt
        receipt['object_corruption']['corrupt_sha256'] = hashlib.sha256(changed_members['object.o']).hexdigest()
        receipt['object_corruption']['bundle_corrupt_sha256'] = sha(victim)
        receipt['object_corruption']['receipt_bytes_unchanged'] = True
        save()
        rebound = build('entry-default-corrupt-rebound', 'entry', False, cache)
        assert rebound['result']['corrupt_rebuilt'] == 1 and rebound['result']['cache_misses'] == 1
        assert rebound['result']['cache_hits'] + 1 == rebound['result']['current_preflight_units']
        assert hashlib.sha256(read_bundle(victim)['object.o']).hexdigest() == receipt['object_corruption']['before_sha256']
        assert [k['action_key'] for k in rebound['keys']] == [k['action_key'] for k in warm['keys']]

        edited = original.replace('fn padding300(x:i32)->i32{return x+1;}',
                                  'fn padding300(x:i32)->i32{return x+2;}')
        assert edited != original
        working.write_text(edited)
        edit = build('entry-default-private-edit', 'entry', False, cache)
        assert edit['result']['cache_misses'] > 0 and edit['result']['cache_hits'] > 0
        assert {k['action_key'] for k in edit['keys']} != {k['action_key'] for k in warm['keys']}

        # The direct compiler request must preserve the existing valid artifacts
        # on a current-source type rejection. It is intentionally not run through
        # the success-only publication step in run_request.py.
        check_frozen()
        bad = root / 'invalid-current-source'
        bad.mkdir()
        invalid = edited.replace('f()+p(0)-67', 'f()+p(0,1)-67')
        assert invalid != edited
        working.write_text(invalid)
        (root / 'sources/invalid-current-source.zil').write_text(invalid)
        valid_out = Path(edit['output'])
        old = {str(p): sha(p) for p in [valid_out / 'program', valid_out / 'program.ll']}
        before_cache = inventory(cache)
        config = json.loads((valid_out / 'config.json').read_text())
        config['request'] = str(bad)
        (bad / 'config.json').write_text(json.dumps(config, indent=2) + '\n')
        command = json.loads((valid_out / 'manifest.json').read_text())['command']
        env = os.environ.copy()
        env.update(ZIL_JT_CONFIG=str(bad / 'config.json'), ZIL_THIN_CAPTURE=str(bad / 'units'),
                   ZIL_THIN_IMPORTS_OFF='1')
        run = subprocess.run(command, env=env, capture_output=True)
        (bad / 'driver.stdout').write_bytes(run.stdout)
        (bad / 'driver.stderr').write_bytes(run.stderr)
        report = json.loads(run.stdout)
        invalid_receipt = {'command': command, 'returncode': run.returncode, 'report': report,
                           'source_sha256': sha(working), 'old_artifacts_before': old,
                           'old_artifacts_after': {p: sha(Path(p)) for p in old},
                           'cache_before': before_cache, 'cache_after': inventory(cache),
                           'pending_objects': list(map(str, (bad / 'pending').glob('**/*'))) if (bad / 'pending').exists() else [],
                           'unit_inputs': list(map(str, bad.glob('unit-*.ll.input.json')))}
        invalid_receipt['valid'] = (run.returncode != 0 and report.get('status') == 'SafeReject'
                                    and report.get('phase') == 'types'
                                    and not report.get('native_codegen')
                                    and old == invalid_receipt['old_artifacts_after']
                                    and before_cache == invalid_receipt['cache_after']
                                    and not invalid_receipt['pending_objects']
                                    and not invalid_receipt['unit_inputs'])
        (bad / 'result.json').write_text(json.dumps(invalid_receipt, indent=2) + '\n')
        receipt['invalid_source'] = {'result': str(bad / 'result.json'), 'valid': invalid_receipt['valid']}
        assert invalid_receipt['valid'], 'invalid source replaced artifacts or mutated cache'
        receipt['valid'] = True
    except Exception as error:
        receipt['error'] = repr(error)
        raise
    finally:
        working.write_text(original)
        receipt['working_source_restored'] = True
        save()
        print(json.dumps({'valid': receipt['valid'], 'successful_builds': sum(r.get('valid', False) for r in receipt['rows']),
                          'receipt': str(root / 'result.json'), 'error': receipt.get('error')}), flush=True)


if __name__ == '__main__':
    main()
