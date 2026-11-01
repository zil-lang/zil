#!/usr/bin/env python3
"""Capture unchanged actual API modules; diagnostic capture is not a speed result."""
import argparse
import json
import os
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from stage6_toolchain_runtime import command_run, clean_environment, identity, save


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--headers', type=Path, required=True)
    parser.add_argument('--case', action='append', choices=['8192-arithmetic', '2048-branch', '512-body'])
    parser.add_argument('--closed', action='store_true')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((args.prepared / 'manifest.json').read_text())
    source = Path(__file__).with_name('capture.c')
    clang = Path('/workspace/.zil-env/llvm-23.1.4/bin/clang').resolve()
    library = Path('/workspace/.zil-env/research/stage6/combined-v1-candidate/lib/libLLVM.so.23.1').resolve()
    shim = args.out / 'capture.so'
    receipt = dict(status='Running', scope=__doc__, capture_source=identity(source),
                   runner=identity(__file__), clang=identity(clang), llvm=identity(library),
                   prepared_manifest=identity(args.prepared / 'manifest.json'), requests=[])
    save(args.out / 'receipt.json', receipt)
    try:
        build = command_run([clang, '-std=c11', '-O2', '-fPIC', '-shared',
                             '-I' + str(args.headers / 'include'), source, '-ldl', '-o', shim],
                            args.out / 'shim-build', clean_environment(), 30)
        if build['exit'] != 0:
            raise RuntimeError('shim build failed')
        receipt['shim'] = identity(shim)
        for case in args.case or ['8192-arithmetic', '2048-branch']:
            directory = args.out / case
            units = directory / 'units'
            units.mkdir(parents=True)
            original = args.prepared / case / ('entry_incremental_after_closed' if args.closed else 'entry_incremental_after')
            command = json.loads((original / 'build.process.json').read_text())['command']
            compiler = Path(command[0]).resolve()
            command[command.index('-o') + 1] = str(directory / 'program')
            command += ['--object-cache', str(directory / 'unused-cache')]
            environment = clean_environment()
            environment.update(LD_LIBRARY_PATH=str(library.parent), LD_PRELOAD=str(shim),
                ZIL_CLANG=str(clang), ZIL_CGU_CAPTURE_DIR=str(units),
                ZIL_CGU_CAPTURE_EXECUTABLE=str(compiler), ZIL_CGU_CAPTURE_LLVM=str(library))
            request = dict(case=case, compiler=identity(compiler), source=identity(args.prepared / case / 'main.zil'),
                           environment_overrides={k: environment[k] for k in ['LD_LIBRARY_PATH', 'LD_PRELOAD',
                               'ZIL_CLANG', 'ZIL_CGU_CAPTURE_DIR', 'ZIL_CGU_CAPTURE_EXECUTABLE', 'ZIL_CGU_CAPTURE_LLVM']})
            receipt['requests'].append(request)
            request['build'] = command_run(command, directory / 'build', environment, 180)
            if request['build']['exit'] != 0:
                raise RuntimeError('actual API build failed: ' + case)
            result = json.loads((directory / 'build.stdout').read_text())
            codegen = result['native_codegen']
            cache = codegen['object_cache']
            request['checks'] = dict(success=result['status'] == 'Success', verify=result['verification_runs'] == 1,
                api=codegen['backend'] == 'llvm-api', units=codegen['actual_units'] == (3 if case == '512-body' else 8),
                current_verified=all(row['verify_before_after'] for row in codegen['object_reports']),
                cache_disabled=cache['requested'] and not cache['enabled'] and 'interposition' in cache['disabled_reason'],
                capture_count=len(list(units.glob('*.before.ll'))) == codegen['actual_units'] and
                    len(list(units.glob('*.after.ll'))) == codegen['actual_units'])
            request['executable'] = identity(directory / 'program')
            request['frozen_executable'] = identity(original / 'program')
            request['checks']['byte_equal_frozen'] = request['executable']['sha256'] == request['frozen_executable']['sha256']
            request['native'] = []
            for seed in manifest['seeds']:
                expected = manifest['expected'][case][str(seed)].to_bytes(4, 'little')
                run = command_run([directory / 'program'], directory / ('native-' + str(seed)), clean_environment(), 15, bytes([seed]))
                row = dict(seed=seed, expected_hex=expected.hex(), process=run,
                    valid=run['exit'] == 0 and Path(run['stdout_path']).read_bytes() == expected and not Path(run['stderr_path']).read_bytes())
                request['native'].append(row)
            request['captures'] = [identity(path) for path in sorted(units.iterdir())]
            request['valid'] = all(request['checks'].values()) and all(row['valid'] for row in request['native'])
            save(args.out / 'receipt.json', receipt)
            print(case, request['checks'], 'native', sum(row['valid'] for row in request['native']), flush=True)
        receipt['status'] = 'Pass' if all(row['valid'] for row in receipt['requests']) else 'Fail'
    except BaseException as error:
        receipt.update(status='Error', error=repr(error))
        raise
    finally:
        save(args.out / 'receipt.json', receipt)


if __name__ == '__main__':
    main()
