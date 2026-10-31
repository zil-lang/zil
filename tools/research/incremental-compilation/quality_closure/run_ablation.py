#!/usr/bin/env python3
"""Replay bounded/full AE/internal implementation clones; no compile-speed claims."""
import argparse
import json
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from stage6_toolchain_runtime import command_run, clean_environment, identity, save
from prepare_ablation import functions


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=Path, required=True)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--generated', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--case', action='append', choices=['8192-arithmetic', '2048-branch', '512-body'])
    parser.add_argument('--arm', action='append', choices=['bounded-ae', 'full-ae', 'bounded-internal', 'full-internal'])
    parser.add_argument('--closed', action='store_true')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    captured = json.loads((args.capture / 'receipt.json').read_text())
    generated = json.loads((args.generated / 'receipt.json').read_text())
    prepared = json.loads((args.prepared / 'manifest.json').read_text())
    library, clang = captured['llvm'], captured['clang']
    assert identity(library['path']) == library and identity(clang['path']) == clang
    settings = dict(triple='x86_64-unknown-linux-gnu', cpu='x86-64', features='', codegen_level=2,
        relocation=2, code_model=0, pipeline='default<O2>', verify_each=0, loop_interleaving=1,
        loop_vectorization=1, slp_vectorization=1, merge_functions=1)
    receipt = dict(status='Running', scope=__doc__, producer=identity(__file__),
                   child=identity(HERE / 'replay_child.py'), settings=settings, cases=[])
    environment = clean_environment()
    environment['LD_LIBRARY_PATH'] = str(Path(library['path']).parent)
    try:
        for supplied in generated['cases']:
            case = supplied['case']
            if args.case and case not in args.case:
                continue
            directory = args.out / case
            directory.mkdir()
            frozen = args.prepared / case / ('entry_incremental_after_closed' if args.closed else 'entry_incremental_after')
            report = json.loads((frozen / 'build.stdout').read_text())
            assert report['verification_runs'] == 1 and report['status'] == 'Success'
            unit_count = report['native_codegen']['actual_units']
            assert unit_count == (3 if case == '512-body' else 8)
            for unit in report['native_codegen']['object_reports']:
                assert unit['backend'] == 'llvm-api' and unit['triple'] == settings['triple']
                assert unit['cpu'] == settings['cpu'] and unit['features'] == settings['features']
                assert unit['relocation'] == 'PIC' and unit['code_model'] == 'default'
                assert unit['verify_before_after'] and unit['pipeline'].startswith('default<O2>|merge-functions=after|aliases=false|')
                assert unit['pipeline'].endswith('|dso-sha256=' + library['sha256'])
            sources = {}
            for original in supplied['original_units']:
                path = Path(original['path'])
                assert identity(path)['sha256'] == original['sha256']
                bodies = functions(path.read_text())
                ids = [int(name.rsplit('.', 1)[1]) for name, body in bodies.items()
                       if name.startswith('zil.fn.') and 'available_externally ' not in body.splitlines()[0]]
                owners = {index // 256 % 8 for index in ids}
                assert len(owners) == 1, ('not the fixed single-source chunk distribution', path, owners)
                owner = owners.pop()
                assert owner not in sources
                sources[owner] = path
            assert set(sources) == set(range(unit_count))
            root_owner = next(owner for owner, path in sources.items() if path == Path(supplied['original_root']['path']))
            case_row = dict(case=case, frozen_build_report=identity(frozen / 'build.stdout'),
                root_owner=root_owner, unit_mapping={str(owner): identity(path) for owner, path in sources.items()}, arms=[])
            receipt['cases'].append(case_row)
            selected_arms = args.arm or ['bounded-ae', 'full-ae', 'bounded-internal', 'full-internal']
            assert selected_arms[0] == 'bounded-ae'
            for arm in selected_arms:
                arm_dir = directory / arm
                arm_dir.mkdir()
                if arm == 'bounded-ae':
                    inputs = [dict(name=f'unit-{owner}', **identity(path)) for owner, path in sorted(sources.items())]
                else:
                    path = Path(supplied['variants'][arm]['path'])
                    assert identity(path)['sha256'] == supplied['variants'][arm]['sha256']
                    inputs = [dict(name=f'unit-{root_owner}', **identity(path))]
                spec = arm_dir / 'spec.json'
                save(spec, dict(library=library, settings=settings, inputs=inputs))
                row = dict(arm=arm, inputs=inputs, valid=False)
                case_row['arms'].append(row)
                child = command_run([sys.executable, HERE / 'replay_child.py', spec, arm_dir / 'api'],
                                    arm_dir / 'child', environment, 180)
                row['child'] = child
                assert child['exit'] == 0, ('C API replay failed', case, arm)
                api = json.loads((arm_dir / 'api/receipt.json').read_text())
                assert api['status'] == 'Pass'
                objects = [((arm_dir / 'api') if arm == 'bounded-ae' or owner == root_owner
                    else directory / 'bounded-ae/api') / f'unit-{owner}.o' for owner in range(unit_count)]
                row['objects'] = [identity(path) for path in objects]
                original_command = report['commands'][-1]
                assert original_command[1:3] == ['--target=x86_64-unknown-linux-gnu', '-O2']
                assert original_command[0] == clang['path']
                assert original_command[-2] == '-o' and len(original_command) == unit_count + 5
                link = [original_command[0], *original_command[1:3], *objects, '-o', arm_dir / 'program']
                row['link'] = command_run(link, arm_dir / 'link', clean_environment(), 30)
                assert row['link']['exit'] == 0
                row['program'] = identity(arm_dir / 'program')
                row['frozen_program'] = identity(frozen / 'program')
                row['same_frozen_bytes'] = row['program']['sha256'] == row['frozen_program']['sha256']
                if arm == 'bounded-ae':
                    assert row['same_frozen_bytes'], 'baseline replay differs: do not attribute transformed results'
                row['nm'] = command_run(['/usr/bin/nm', '--extern-only', '--defined-only', '--format=posix',
                                         arm_dir / 'program'], arm_dir / 'nm', clean_environment(), 20)
                assert row['nm']['exit'] == 0
                symbols = [line.split() for line in Path(row['nm']['stdout_path']).read_text().splitlines()]
                source_symbols = {item[0]: item[2] for item in symbols if item[0].startswith('zil.fn.')}
                frozen_source_symbols = {item[0] for line in (frozen / 'nm-global.stdout').read_text().splitlines()
                                         if (item := line.split()) and item[0].startswith('zil.fn.')}
                row['canonical_functions'] = len(source_symbols)
                row['canonical_unique_addresses'] = len(set(source_symbols.values())) == len(source_symbols)
                assert set(source_symbols) == frozen_source_symbols
                assert row['canonical_unique_addresses']
                row['native'] = []
                for seed in prepared['seeds']:
                    expected = prepared['expected'][case][str(seed)].to_bytes(4, 'little')
                    native = command_run([arm_dir / 'program'], arm_dir / f'native-{seed}', clean_environment(), 15, bytes([seed]))
                    valid = native['exit'] == 0 and Path(native['stdout_path']).read_bytes() == expected and not Path(native['stderr_path']).read_bytes()
                    row['native'].append(dict(seed=seed, expected_hex=expected.hex(), process=native, valid=valid))
                    assert valid, ('native mismatch', case, arm, seed)
                row['valid'] = True
                save(args.out / 'receipt.json', receipt)
                print(case, arm, 'Pass', 'same_frozen_bytes', row['same_frozen_bytes'], flush=True)
        receipt['status'] = 'Pass'
    except BaseException as error:
        receipt.update(status='Fail', error=repr(error))
        raise
    finally:
        save(args.out / 'receipt.json', receipt)


if __name__ == '__main__':
    main()
