#!/usr/bin/env python3
"""Build the pinned, opt-in function-product LLVM extension in a new prefix.

Only the pinned translation units in the manifest are replaced. The original
SDK and DSO remain unchanged. This is a locally modified LLVM build.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
NATIVE = ROOT / 'crates/zil-llvm-api/native'


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def build(sdk, prefix, receipt):
    manifest = json.loads((NATIVE / 'llvm-function-products.json').read_text())
    installed = json.loads((sdk / 'install-receipt.json').read_text())
    if installed['status'] != 'Pass' or installed['release']['version'] != manifest['version']:
        raise ValueError('SDK receipt/version mismatch')
    patch = NATIVE / 'llvm-function-products.patch'
    if sha(patch) != manifest['patch_sha256']:
        raise ValueError('patch differs from its pinned manifest')
    if sha(sdk / installed['library']['path']) != installed['library']['sha256']:
        raise ValueError('original SDK DSO no longer matches its receipt')
    logs = prefix / 'logs'
    logs.mkdir()
    receipt.update({'llvm_commit': manifest['llvm_commit'], 'version': manifest['version'],
                    'patch_sha256': sha(patch), 'hook_header_sha256': sha(NATIVE / 'zil_llvm_work_products_hooks.h'),
                    'sdk_receipt_sha256': sha(sdk / 'install-receipt.json'),
                    'original_dso_sha256': installed['library']['sha256'], 'commands': []})

    def run(name, args, cwd=None):
        record = {'name': name, 'argv': [str(x) for x in args]}
        receipt['commands'].append(record)
        with (logs / (name + '.stdout')).open('xb') as out, (logs / (name + '.stderr')).open('xb') as err:
            result = subprocess.run(record['argv'], cwd=cwd, stdout=out, stderr=err, timeout=900)
        record['exit_code'] = result.returncode
        for stream in ['stdout', 'stderr']:
            path = logs / (name + '.' + stream)
            record[stream] = {'path': str(path.relative_to(prefix)), 'sha256': sha(path)}
        if result.returncode:
            raise RuntimeError(f'{name} failed: {result.returncode}; see {logs}')
        return (logs / (name + '.stdout')).read_text()

    source_root = prefix / 'source'
    for index, (relative, expected) in enumerate(manifest['original_files'].items()):
        path = source_root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        url = f'https://raw.githubusercontent.com/llvm/llvm-project/{manifest["llvm_commit"]}/{relative}'
        run(f'source-download-{index}', ['curl', '--fail', '--location', '--retry', '3', '--max-time', '120', url, '-o', path])
        if sha(path) != expected:
            raise ValueError('upstream source hash mismatch')
    run('apply-patch', ['patch', '--batch', '--fuzz=0', '-p1', '-i', patch], source_root)
    config = sdk / 'bin/llvm-config'
    cxxflags = shlex.split(run('cxxflags', [config, '--cxxflags']))
    lib = prefix / 'lib'
    lib.mkdir()
    units = manifest.get('compile_units', [{
        'source': 'llvm/lib/CodeGen/MachineFunctionPass.cpp',
        'archive': 'libLLVMCodeGen.a', 'member': 'MachineFunctionPass.cpp.o',
    }])
    if not units:
        raise ValueError('empty replacement set')
    replacements = {}
    seen = set()
    receipt['compile_units'] = []
    for index, unit in enumerate(units):
        source_name, archive, member = (unit[key] for key in ('source', 'archive', 'member'))
        if source_name not in manifest['original_files']:
            raise ValueError('unbound translation unit: ' + source_name)
        if Path(archive).name != archive or Path(member).name != member:
            raise ValueError('archive and member must be basenames')
        if (archive, member) in seen:
            raise ValueError('duplicate replacement member')
        seen.add((archive, member))
        source = source_root / source_name
        object_dir = prefix / f'objects-{index}'
        object_dir.mkdir()
        obj = object_dir / member
        includes = []
        for include in unit.get('include_dirs', []):
            include_path = (source_root / include).resolve()
            include_path.relative_to(source_root)
            includes.extend(['-I', include_path])
        run(f'compile-extension-{index}', [sdk / 'bin/clang++', *cxxflags, '-O3', '-DNDEBUG', '-fPIC',
                                          '-I', NATIVE, *includes, *unit.get('flags', []),
                                          '-c', source, '-o', obj])
        replacement = lib / archive
        if archive not in replacements:
            relative = 'lib/' + archive
            if sha(sdk / relative) != installed['component_inputs'][relative]:
                raise ValueError('original component changed: ' + archive)
            shutil.copyfile(sdk / relative, replacement)
            replacements[archive] = str(replacement)
        original_members = run(f'original-members-{index}', [sdk / 'bin/llvm-ar', 't', replacement]).splitlines()
        if original_members.count(member) != 1:
            raise ValueError('original member is missing or ambiguous: ' + member)
        run(f'replace-object-{index}', [sdk / 'bin/llvm-ar', 'r', replacement, obj])
        members = run(f'archive-members-{index}', [sdk / 'bin/llvm-ar', 't', replacement]).splitlines()
        if members != original_members:
            raise ValueError('replacement changed component member order or membership')
        receipt['compile_units'].append({**unit, 'patched_source_sha256': sha(source),
                                          'object_sha256': sha(obj)})
    libraries = shlex.split(run('component-libraries', [config, '--link-static', '--libfiles', *installed['release']['components']]))
    libraries = list(dict.fromkeys(libraries))
    for name in libraries:
        relative = str(Path(name).relative_to(sdk))
        if sha(Path(name)) != installed['component_inputs'][relative]:
            raise ValueError(f'component differs from original SDK receipt: {relative}')
    if not set(replacements).issubset({Path(name).name for name in libraries}):
        raise ValueError('replacement component is not in the linked SDK component set')
    libraries = [replacements.get(Path(name).name, name) for name in libraries]
    major, minor, _ = manifest['version'].split('.')
    soname = f'libLLVM.so.{major}.{minor}'
    version_map = prefix / 'llvm.map'
    version_map.write_text(f'LLVM_{major}.{minor} {{ global: *; }};\n')
    output = lib / soname
    run('link-dso', [sdk / 'bin/clang++', '-shared', '-fuse-ld=lld', '-Wl,-z,defs', '-Wl,-z,text',
                    '-Wl,-Bsymbolic-functions', '-Wl,--build-id=sha1', '-Wl,-soname,' + soname,
                    '-Wl,--version-script=' + str(version_map), '-Wl,--whole-archive', *libraries,
                    '-Wl,--no-whole-archive', *installed['linked_system_libraries'], '-o', output])
    (lib / f'libLLVM-{major}.so').symlink_to(soname)
    loaded = ctypes.CDLL(str(output))
    values = [ctypes.c_uint() for _ in range(3)]
    loaded.LLVMGetVersion.argtypes = [ctypes.POINTER(ctypes.c_uint)] * 3
    loaded.LLVMGetVersion.restype = None
    loaded.LLVMGetVersion(*(ctypes.byref(v) for v in values))
    if '.'.join(str(v.value) for v in values) != manifest['version']:
        raise ValueError('replacement DSO version mismatch')
    loaded.ZilLLVMGetFunctionProductHooks.restype = ctypes.c_void_p
    if loaded.ZilLLVMGetFunctionProductHooks() is not None:
        raise ValueError('extension unexpectedly enabled without an explicit client')
    dependencies = run('dependencies', ['ldd', output])
    if 'not found' in dependencies:
        raise ValueError('replacement DSO has missing dependencies')
    for symbol in manifest.get('required_exports', []):
        if not hasattr(loaded, symbol):
            raise ValueError('missing extension export: ' + symbol)
    receipt['replacement_archives'] = {name: sha(Path(path)) for name, path in replacements.items()}
    receipt['library'] = {'path': str(output), 'sha256': sha(output)}
    receipt['default_hook'] = 'null: original pipeline selected'
    env = {'ZIL_LLVM_LIB_DIR': str(lib), 'ZIL_LLVM_INCLUDE_DIR': str(sdk / 'include'),
           'ZIL_LLVM_CONFIG': str(config), 'ZIL_CLANG': str(sdk / 'bin/clang')}
    (prefix / 'env.sh').write_text(''.join(f'export {key}={shlex.quote(value)}\n' for key, value in env.items()) +
                                  f'export LD_LIBRARY_PATH={shlex.quote(str(lib))}${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sdk', type=Path, required=True)
    parser.add_argument('--prefix', type=Path, required=True, help='new absolute output directory')
    args = parser.parse_args()
    sdk = args.sdk.resolve(strict=True)
    if not args.prefix.is_absolute() or args.prefix.exists():
        parser.error('--prefix must be a new absolute path')
    prefix = args.prefix.resolve()
    prefix.mkdir(parents=True)
    receipt = {'status': 'Fail', 'script_sha256': sha(Path(__file__))}
    try:
        build(sdk, prefix, receipt)
        receipt['status'] = 'Pass'
    except (OSError, KeyError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        receipt['error'] = str(error)
    (prefix / 'build-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({'status': receipt['status'], 'receipt': str(prefix / 'build-receipt.json'), 'error': receipt.get('error')}))
    return 0 if receipt['status'] == 'Pass' else 1


if __name__ == '__main__':
    raise SystemExit(main())
