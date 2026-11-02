#!/usr/bin/env python3
"""Optimize actual preflight-checked CGUs with a pinned opt plugin, then emit once."""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import zipfile
from pathlib import Path
from cache_bundle import read_bundle


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    config_path = Path(os.environ['ZIL_JT_CONFIG'])
    config = json.loads(config_path.read_text())
    root, cache = Path(config['request']), Path(config['cache'])
    original = sys.argv[1:]

    def tool(name):
        item = config['tools'][name]
        path = Path(item['path'])
        stat = path.stat()
        assert stat.st_size == item['bytes'] and stat.st_mtime_ns == item['mtime_ns'], name+' changed'
        return str(path)

    clang, opt, plugin, lld = (tool(name) for name in ['clang', 'opt', 'plugin', 'lld'])
    tool('zil')

    def invoke(command, label):
        start = time.perf_counter_ns()
        stdout, stderr, exit_code, error = b'', b'', None, None
        try:
            run = subprocess.run(command, capture_output=True)
            stdout, stderr, exit_code = run.stdout, run.stderr, run.returncode
        except OSError as failure:
            error = {'kind': type(failure).__name__, 'message': str(failure)}
            stderr = (json.dumps(error)+'\n').encode()
        finally:
            (root/(label+'.stdout')).write_bytes(stdout)
            (root/(label+'.stderr')).write_bytes(stderr)
            (root/(label+'.command.json')).write_text(json.dumps({
                'original_command': original, 'actual_command': command,
                'exit': exit_code, 'error': error, 'elapsed_ns': time.perf_counter_ns()-start,
                'config_sha256': sha(config_path)}, indent=2)+'\n')
        sys.stdout.buffer.write(stdout)
        sys.stderr.buffer.write(stderr)
        if error or exit_code:
            raise SystemExit(exit_code or 1)

    def compile_unit(source, output):
        name = source.name
        assert name.startswith('unit-') and name.endswith('.ll'), 'requires actual CGU, not public program.ll'
        captured = root/'units'/name
        assert captured.read_bytes() == source.read_bytes(), 'actual compiler input differs from current preflight capture'
        unit = int(name.removeprefix('unit-').removesuffix('.ll'))
        preflight = json.loads((root/'units/preflight.json').read_text())
        assert preflight[unit]['unit'] == unit and preflight[unit]['machine_preflight_current'] is True
        lines = captured.read_text().splitlines(keepends=True)
        assert lines[0] == '; ZIL verified LLVM module\n' and lines[1].startswith('; verified-fingerprint: ')
        lines[1] = '; verified-fingerprint: current-request-preflight-recorded-separately\n'
        normalized = ''.join(lines).encode()
        normalized_path = root/(name+'.normalized.ll')
        normalized_path.write_bytes(normalized)
        pipeline = config['pipeline']
        if config['verify_analyses']:
            node = {'builtin': 'jump-threading', 'original': 'zil-jt-original', 'entry': 'zil-jt-entry'}[config['arm']]
            pipeline = pipeline.replace(node, node+',verify<domtree>')
        material = {'schema': 2, 'arm': config['arm'], 'pipeline': pipeline,
                    'input_sha256': hashlib.sha256(normalized).hexdigest(),
                    'target_flags': config['target_flags'], 'tools': config['tools'],
                    'wrapper_sha256': sha(Path(__file__)), 'verify_each': config['verify_each'],
                    'cache_protocol_sha256': sha(Path(__file__).with_name('cache_bundle.py')),
                    'diagnostics': config['diagnostics'],
                    'codegen': ['-x', 'ir', '-O2', '-march=x86-64', '-fPIC', '-Xclang', '-disable-llvm-passes', '-ffunction-sections', '-fdata-sections'],
                    'cache_scope': 'Objects only. Linking always runs; machine preflight is current on every hit.'}
        key = hashlib.sha256(json.dumps(material, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        entry = cache/(key+'.zip')
        hit, corrupt = False, False
        payload = None
        if entry.exists():
            try:
                payload = read_bundle(entry, material)
                hit = True
            except (OSError, ValueError, KeyError, TypeError, UnicodeError, RuntimeError, zipfile.BadZipFile):
                hit = False
            corrupt = not hit
        optimized = root/(name+'.optimized.bc')
        if hit:
            output.write_bytes(payload[1])
            optimized.write_bytes(payload[2])
        else:
            command = [opt, '-load-pass-plugin='+plugin, *config['target_flags'],
                       '-passes='+pipeline, str(normalized_path), '-o', str(optimized)]
            if config['diagnostics']:
                # opt registers plugin CLI options when load-pass-plugin is
                # encountered; a plugin-specific flag must follow that flag.
                command[2:2] = ['-zil-jt-audit', '-time-passes']
            if config['verify_each']:
                command.insert(1, '-verify-each')
            invoke(command, name+'.opt')
            assert optimized.read_bytes().startswith(b'BC\xc0\xde')
            # Backend optimization remains O2; the IR pipeline must not run a second time.
            command = [clang, '--target=x86_64-unknown-linux-gnu', *material['codegen'],
                       '-c', str(optimized), '-o', str(output)]
            invoke(command, name+'.codegen')
            pending = root/'pending'/key
            pending.mkdir(parents=True)
            shutil.copy2(output, pending/'object.o')
            shutil.copy2(optimized, pending/'optimized.bc')
            (pending/'receipt.json').write_text(json.dumps({'material': material,
                'object_sha256': sha(output), 'bitcode_sha256': sha(optimized)}, indent=2)+'\n')
        shutil.copy2(output, root/(name+'.o'))
        (root/(name+'.input.json')).write_text(json.dumps({'unit': unit, 'raw_sha256': sha(captured),
            'normalized_sha256': material['input_sha256'], 'action_key': key, 'material': material,
            'cache_entry_path': str(entry), 'cache_storage': 'atomic ZIP_STORED bundle, schema2',
            'cache_hit': hit, 'corrupt_rebuilt': corrupt, 'object_sha256': sha(output),
            'bitcode_sha256': sha(optimized), 'current_machine_preflight': True,
            'original_command': original}, indent=2)+'\n')

    def link(objects, output):
        symbols = [x for x in (root/'units/exports.txt').read_text().splitlines() if x]
        exports = root/'exports.list'
        exports.write_text('{\n'+''.join('  '+json.dumps(x)+';\n' for x in symbols)+'};\n')
        command = [clang, '--target=x86_64-unknown-linux-gnu', '-fuse-ld='+lld,
                   *map(str, objects), '-o', str(output), '-Wl,--gc-sections',
                   '-Wl,--export-dynamic-symbol-list='+str(exports)]
        invoke(command, 'link')
        (root/'link-inputs.json').write_text(json.dumps({'objects': [{'path': str(p), 'sha256': sha(p)} for p in objects],
            'exports': symbols, 'command': command, 'link_cache_used': False,
            'output_sha256': sha(output)}, indent=2)+'\n')

    index = original.index('-o')
    output = Path(original[index+1])
    inputs = [Path(x) for i, x in enumerate(original) if i != index+1 and x.endswith(('.ll', '.o'))]
    assert '--target=x86_64-unknown-linux-gnu' in original
    if '-c' in original:
        assert len(inputs) == 1 and inputs[0].suffix == '.ll'
        compile_unit(inputs[0], output)
    elif any(p.suffix == '.ll' for p in inputs):
        assert len(inputs) == 1
        obj = root/'single-input.o'
        compile_unit(inputs[0], obj)
        link([obj], output)
    else:
        assert inputs and all(p.suffix == '.o' for p in inputs)
        link(inputs, output)


if __name__ == '__main__':
    try:
        main()
    except BaseException as failure:
        if isinstance(failure, SystemExit) and not failure.code:
            raise
        error = {'kind': type(failure).__name__, 'message': str(failure),
                 'original_command': sys.argv[1:]}
        try:
            config_path = Path(os.environ['ZIL_JT_CONFIG'])
            config = json.loads(config_path.read_text())
            root = Path(config['request'])
            (root/f'wrapper-failure-{os.getpid()}.json').write_text(json.dumps(error, indent=2)+'\n')
        except (OSError, ValueError, KeyError, TypeError):
            pass
        sys.stderr.write(json.dumps(error)+'\n')
        raise SystemExit(1)
