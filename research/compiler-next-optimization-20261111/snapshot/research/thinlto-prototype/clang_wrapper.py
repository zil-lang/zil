#!/usr/bin/env python3
"""Only the pinned research exporter may invoke this Clang replacement."""
import json
import os
from pathlib import Path
import shutil
import sys
import time
import traceback
from common import *
import common
common.OWN_GROUP = False

def main():
    protocol = Path(os.environ['ZIL_THINLTO_PROTOCOL'])
    p = json.loads(protocol.read_text())
    if sha(protocol) != os.environ['ZIL_THINLTO_PROTOCOL_SHA']:
        raise ValueError('protocol changed')
    validate(p)
    if set(os.sched_getaffinity(0)) != set(p['cpus']):
        raise ValueError('wrapper must inherit exact frozen CPU affinity')
    preflight(os.environ['ZIL_RESEARCH_THINLTO_RECEIPT'])
    logroot = Path(os.environ['ZIL_THINLTO_LOGDIR'])
    logroot.mkdir(parents=True, exist_ok=True)
    args = sys.argv[1:]
    label = f'{time.time_ns()}-{os.getpid()}'
    log = logroot / label
    log.mkdir()
    write(log / 'invocation.json', {'original_argv': args, 'actual_affinity': sorted(os.sched_getaffinity(0)),
                                  'protocol_sha256': sha(protocol)})
    if args.count('-o') != 1 or '-O2' not in args or '--target=x86_64-unknown-linux-gnu' not in args:
        raise ValueError('unexpected compiler command')
    output = Path(args[args.index('-o') + 1])
    clang, opt = p['tools']['clang']['path'], p['tools']['opt']['path']
    if '-c' in args:
        if args.count('-c') != 1 or args.count('-x') != 1 or args[args.index('-x') + 1] != 'ir':
            raise ValueError('expected one IR compile')
        inputs = [Path(v) for v in args if v.endswith('.ll')]
        if len(inputs) != 1 or not inputs[0].is_file():
            raise ValueError('expected actual current unit IR')
        source = log / 'input.ll'
        shutil.copy2(inputs[0], source)
        run([opt, '-passes=verify', '-disable-output', source], log / 'verify-before')
        actual = args + ['-flto=thin', '-march=x86-64', '-fPIC']
        run([clang] + actual, log / 'thin-prelink')
        bitcode = log / 'prelink.thin.bc'
        shutil.copy2(output, bitcode)
        run([opt, '-passes=verify', '-disable-output', bitcode], log / 'verify-prelink')
        run([p['tools']['bcanalyzer']['path'], '-dump', bitcode], log / 'summary-dump')
        dump = (log / 'summary-dump' / 'stdout').read_text()
        if 'GLOBALVAL_SUMMARY_BLOCK' not in dump or '<HASH ' not in dump:
            raise ValueError('actual prelink bitcode lacks summary/module hash')
        run([p['tools']['dis']['path'], bitcode, '-o', log / 'prelink.ll'], log / 'disassemble-prelink')
        ir = (log / 'prelink.ll').read_text()
        if 'target triple = "x86_64-unknown-linux-gnu"' not in ir:
            raise ValueError('actual prelink target differs from API')
        write(log / 'receipt.json', {'kind': 'unit-prelink', 'input': identity(source),
              'original_output': str(output), 'bitcode': identity(bitcode),
              'summary_block': True, 'module_hash_record': True,
              'verify_before': True, 'verify_prelink_after': True,
              'actual_command': [clang] + actual, 'cpu': 'x86-64', 'relocation': 'PIC',
              'code_model': 'default (no override)',
              'prelink_PIC_Level2_present': '!"PIC Level", i32 2' in ir,
              'PIC_codegen_enforcement': 'explicit -pie at actual lld link, lld23 createConfig Reloc::PIC_'})
    else:
        if '-x' in args or '-flto' in args or any(v.startswith('-fuse-ld') for v in args):
            raise ValueError('unexpected link options')
        objects = [Path(v) for v in args if v.endswith(('.o', '.obj'))]
        if len(objects) != 8 or any(not v.is_file() for v in objects):
            raise ValueError('exact eight actual canonical unit bitcodes required')
        cache = logroot / 'native-cache'
        cache.mkdir(exist_ok=True)
        before = sorted(v.name for v in cache.iterdir() if v.is_file())
        # --ld-path pins the linker executable, not just its searched basename.
        actual = [clang] + args + ['-flto=thin', '-fuse-ld=lld', '--ld-path=' + p['tools']['lld']['path'],
                  '-march=x86-64', '-fPIC', '-pie',
                  '-Wl,--lto-O2,--lto-CGO2,--plugin-opt=mcpu=x86-64,--thinlto-jobs=2,--threads=2,--icf=none,--save-temps',
                  '-Wl,--thinlto-emit-imports-files,--thinlto-emit-index-files',
                  '-Wl,--mllvm=-enable-merge-functions=true,--mllvm=-mergefunc-use-aliases=false']
        actual += ['-Wl,--thinlto-cache-dir=' + str(cache)]
        saved = log / 'saved'
        saved.mkdir()
        # ZIL's staging directory disappears when the request returns. Copy all
        # LTO temporaries now, while the actual linker invocation still owns it.
        roots = {v.parent for v in objects} | {output.parent}
        paths = []
        try:
            run(actual, log / 'thin-link', timeout=600)
        finally:
            # Preserve partial optimizer output even when linking fails.
            originals = sorted({v for root in roots for v in root.rglob('*')
                                if v.is_file() and (v.suffix in ['.bc', '.imports'] or v in objects)})
            for index, path in enumerate(originals):
                dest = saved / (f'{index:03d}-' + path.name)
                shutil.copy2(path, dest)
                paths.append({'original': str(path), **identity(dest)})
            write(log / 'saved-files.json', paths)
        optimized = [v for v in saved.iterdir() if v.name.endswith('.4.opt.bc')]
        if len(optimized) != 8:
            raise ValueError('missing eight actual post-import optimized modules')
        for index, path in enumerate(sorted(optimized)):
            run([opt, '-passes=verify', '-disable-output', path], log / f'verify-postlink-{index}')
        imports = [v for v in saved.iterdir() if v.suffix == '.imports']
        if len(imports) != 8:
            raise ValueError('missing actual eight ThinLTO imports files')
        write(log / 'receipt.json', {'kind': 'thin-link', 'actual_command': actual,
              'postlink_optimized_modules_verified': 8, 'saved': paths,
              'actual_imports': {v.name: v.read_text().splitlines() for v in imports},
              'cache_before': before,
              'cache_after': [identity(v) for v in sorted(cache.iterdir()) if v.is_file()],
              'cache_was_initially_empty': not before,
              'output_sha256_before_ZIL_publication': sha(output)})

if __name__ == '__main__':
    try:
        main()
    except BaseException as error:
        directory = Path(os.environ.get('ZIL_THINLTO_LOGDIR', '.'))
        directory.mkdir(parents=True, exist_ok=True)
        write(directory / f'wrapper-failure-{os.getpid()}.json',
              {'exception': repr(error), 'traceback': traceback.format_exc(), 'argv': sys.argv[1:]})
        raise
