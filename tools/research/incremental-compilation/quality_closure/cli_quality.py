#!/usr/bin/env python3
"""Real-source clone quality and complete edit dependencies; diagnostic timing only."""
import argparse
import json
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from stage6_toolchain_runtime import clean_environment, command_run, identity, save


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-receipt', type=Path, required=True)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    build = json.loads(args.build_receipt.read_text())
    assert build['status'] == 'Pass'
    compiler, clang, library = [build[key] for key in ['compiler', 'clang', 'library']]
    for item in [compiler, clang, library]:
        assert identity(item['path'])['sha256'] == item['sha256']
    source_receipt = json.loads(Path(build['source']['path']).read_text())
    assert identity(build['source']['path'])['sha256'] == build['source']['sha256']
    prepared = json.loads((args.prepared / 'manifest.json').read_text())
    receipt = dict(status='Running', scope=__doc__, producer=identity(__file__),
        helper=identity(HERE.parent / 'stage6_toolchain_runtime.py'),
        build_receipt=identity(args.build_receipt), prepared_manifest=identity(args.prepared / 'manifest.json'), rows=[])
    def save_receipt():
        save(args.out / 'receipt.json', receipt)
    try:
        for case in ['8192-arithmetic', '2048-branch', '512-body']:
            count, shape = case.split('-', 1)
            count = int(count)
            original = (args.prepared / case / 'main.zil').read_text()
            for closed in [False, True]:
                directory = args.out / (case + '-closed' + str(int(closed)))
                directory.mkdir()
                source = directory / 'main.zil'
                executable = directory / 'program'
                frozen_arm = 'entry_incremental_after_closed' if closed else 'entry_incremental_after'
                original_command = json.loads((args.prepared / case / frozen_arm / 'build.process.json').read_text())['command']
                expected_units = 3 if count == 512 else 8
                for request, edit in [('cold', None), ('warm', None), ('early_edit', 0), ('late_edit', count - 1), ('restore', None)]:
                    text = original
                    if edit is not None:
                        lines = original.splitlines(True)
                        prefix = f'fn h{edit}('
                        matches = [index for index, line in enumerate(lines) if line.startswith(prefix)]
                        assert len(matches) == 1
                        index = matches[0]
                        before = f'+{edit % 97})%1009'
                        assert before in lines[index]
                        lines[index] = lines[index].replace(before, f'+{edit % 97 + 1})%1009', 1)
                        text = ''.join(lines)
                    source.write_text(text)
                    request_dir = directory / request
                    request_dir.mkdir()
                    command = list(original_command)
                    command[0], command[2] = compiler['path'], str(source)
                    command[command.index('-o') + 1] = str(executable)
                    command += ['--object-cache', str(directory / 'cache')]
                    environment = clean_environment()
                    environment.update(LD_LIBRARY_PATH=str(Path(library['path']).parent),
                        ZIL_CLANG=clang['path'], LD_DEBUG='libs')
                    row = dict(case=case, closed=closed, request=request, edit=edit, source=identity(source), valid=False)
                    receipt['rows'].append(row)
                    row['process'] = command_run(command, request_dir / 'build', environment, 180)
                    assert row['process']['exit'] == 0, (case, closed, request)
                    report = json.loads((request_dir / 'build.stdout').read_text())
                    assert report['status'] == 'Success' and report['verification_runs'] == 1
                    codegen = report['native_codegen']
                    assert codegen['backend'] == 'llvm-api' and codegen['actual_units'] == expected_units
                    assert codegen['fallback'] is None and codegen['object_cache']['enabled']
                    pid = row['process']['pid']
                    loaded = {str(Path(line.split('calling init:', 1)[1].strip()).resolve())
                        for line in Path(row['process']['stderr_path']).read_text().splitlines()
                        if re.match(r'\s*' + str(pid) + r':', line) and 'calling init:' in line and 'libLLVM' in line}
                    assert loaded == {str(Path(library['path']).resolve())}
                    row['loaded_library'] = sorted(loaded)
                    row['cache'] = codegen['object_cache']
                    row['action_keys'] = []
                    for obj in codegen['object_reports']:
                        assert obj['pipeline'].endswith(source_receipt['pipeline_identity_tag'])
                        row['action_keys'].append(obj['cache']['action_key'])
                        if obj['cache']['state'] == 'hit':
                            assert obj['input_kind'] == 'object-cache' and obj['cache']['current_preflight_ns'] > 0
                            assert obj['producer_verified_before_after'] and not obj['llvm_verification_this_request']
                        else:
                            assert obj['input_kind'] == 'builder' and obj['verify_before_after'] and obj['preflight_ns'] > 0
                    misses = (expected_units if request == 'cold' else 0 if request in ['warm', 'restore']
                              else 2 if request == 'late_edit' or count == 512 else 1)
                    assert row['cache']['misses'] == misses, (case, closed, request, row['cache'])
                    assert row['cache']['hits'] == expected_units - misses
                    row['program'] = identity(executable)
                    row['nm'] = command_run(['/usr/bin/nm', '--extern-only', '--defined-only', '--format=posix', executable],
                                             request_dir / 'nm', clean_environment(), 20)
                    assert row['nm']['exit'] == 0
                    symbols = [line.split() for line in Path(row['nm']['stdout_path']).read_text().splitlines()]
                    functions = {item[0]: item[2] for item in symbols if item[0].startswith('zil.fn.')}
                    row['canonical_exports'] = len(functions)
                    if not closed:
                        assert len(functions) == count + 2 and len(set(functions.values())) == len(functions)
                    row['native'] = []
                    for seed in prepared['seeds']:
                        expected = prepared['expected'][case][str(seed)]
                        if edit is not None:
                            x = seed
                            for _ in range(prepared['rounds']):
                                if shape != 'branch' or x % 3 == 0:
                                    value = x + 64 if shape == 'body' else x * 3
                                    expected = (expected + (value + edit % 97 + 1) % 1009 - (value + edit % 97) % 1009) % 1000003
                                x = (x + 17) % 1009
                        native = command_run([executable], request_dir / ('native-' + str(seed)), clean_environment(), 15, bytes([seed]))
                        valid = native['exit'] == 0 and Path(native['stdout_path']).read_bytes() == expected.to_bytes(4, 'little') and not Path(native['stderr_path']).read_bytes()
                        row['native'].append(dict(seed=seed, expected_hex=expected.to_bytes(4, 'little').hex(), process=native, valid=valid))
                        assert valid, (case, closed, request, seed)
                    row['valid'] = True
                    save_receipt()
                    print(case, 'closed', closed, request, 'Pass', 'miss/hit', misses, expected_units-misses, flush=True)
        receipt['status'] = 'Pass'
    except BaseException as error:
        receipt.update(status='Fail', error=repr(error))
        raise
    finally:
        save_receipt()


if __name__ == '__main__':
    main()
