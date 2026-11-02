#!/usr/bin/env python3
"""Replace one native LLVM object and relink a fresh DSO; never rebuild LLVM."""
import argparse
import ctypes
import hashlib
import json
import re
import shlex
import shutil
import subprocess
import time
from pathlib import Path


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def ar_members(path):
    """Read ordinary GNU ar members and hashes without extracting objects."""
    members = {}
    strings = b''
    with path.open('rb') as stream:
        assert stream.read(8) == b'!<arch>\n', 'thin/unknown archive unsupported'
        while header := stream.read(60):
            assert len(header) == 60 and header[58:60] == b'`\n'
            size = int(header[48:58])
            name = header[:16].decode('ascii').strip()
            data = stream.read(size)
            assert len(data) == size
            if size % 2:
                assert len(stream.read(1)) == 1
            if name == '//':
                strings = data
                continue
            if name in ('/', '/SYM64/'):
                continue
            if name.startswith('/') and name[1:].isdigit():
                start = int(name[1:])
                name = strings[start:strings.index(b'/\n', start)].decode()
            else:
                name = name.rstrip('/')
            assert name and name not in members, 'duplicate/invalid archive member'
            members[name] = {'bytes': size, 'sha256': hashlib.sha256(data).hexdigest(),
                             'magic': data[:16].hex()}
    return members


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--llvm-prefix', type=Path, required=True)
    parser.add_argument('--headers-prefix', type=Path, required=True)
    parser.add_argument('--sources', type=Path, required=True)
    parser.add_argument('--arm', choices=['original', 'entry'], required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--jobs', type=int, choices=[1, 2], default=2)
    parser.add_argument('--execute', action='store_true',
                        help='Explicitly perform this bounded build in an authorized functional window')
    args = parser.parse_args()
    if not args.execute:
        raise SystemExit('No work performed; --execute requires a released functional CPU window')
    base = args.llvm_prefix.resolve(strict=True)
    headers = args.headers_prefix.resolve(strict=True)
    sources = args.sources.resolve(strict=True)
    out = args.out.resolve()
    assert out != base and base not in out.parents, 'cannot modify the baseline prefix'
    assert not out.exists(), 'output prefix must be new'
    installed = json.loads((base / 'install-receipt.json').read_text())
    source_manifest = json.loads((sources / 'source-manifest.json').read_text())
    header_manifest = json.loads((headers / 'headers-receipt.json').read_text())
    assert installed['status'] == 'Pass' and installed['release']['version'] == '23.1.4'
    assert header_manifest['status'] == 'Pass'
    assert header_manifest['archive']['sha256'] == installed['release']['archive_sha256']
    assert source_manifest['class_header_layout_unchanged'] is True
    assert source_manifest['upstream']['commit'] == '7a941dfb2877a0b8cf30323fefa85ad019e0b0f5'
    for name, digest in source_manifest['files'].items():
        assert sha(sources / name) == digest, 'generated source changed: ' + name
    assert sha(headers / 'include/llvm/Transforms/Scalar/JumpThreading.h') == source_manifest['header_sha256']
    # Validate all exact generated/public headers before using them in a TU.
    for name, digest in header_manifest['headers'].items():
        assert sha(headers / name) == digest, 'SDK header changed: ' + name
    component_names = list(installed['component_inputs'])
    assert 'lib/libLLVMScalarOpts.a' in component_names
    component_bytes = sum((base / name).stat().st_size for name in component_names)
    existing_parent = out.parent
    while not existing_parent.exists():
        existing_parent = existing_parent.parent
    assert shutil.disk_usage(existing_parent).free > component_bytes + 300_000_000
    out.mkdir(parents=True)
    for name in ['lib', 'objects', 'source', 'logs']:
        (out / name).mkdir()
    receipt = {'status': 'Building', 'scope': __doc__, 'arm': args.arm,
               'baseline_prefix_untouched': True, 'llvm_rebuilt': False,
               'compilation_jobs': 1, 'linker_threads': args.jobs,
               'base_install_receipt_sha256': sha(base / 'install-receipt.json'),
               'source_manifest_sha256': sha(sources / 'source-manifest.json'),
               'headers_receipt_sha256': sha(headers / 'headers-receipt.json'),
               'builder_sha256': sha(Path(__file__)), 'commands': [],
               'component_inputs': {}, 'tools': {}}
    receipt_path = out / 'native-toolchain-receipt.json'

    def save():
        receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')

    def run(label, command):
        record = {'label': label, 'command': [str(x) for x in command]}
        receipt['commands'].append(record)
        save()
        start = time.monotonic()
        with (out / 'logs' / (label + '.stdout')).open('wb') as so, \
                (out / 'logs' / (label + '.stderr')).open('wb') as se:
            result = subprocess.run(record['command'], stdout=so, stderr=se)
        record.update(returncode=result.returncode, diagnostic_elapsed_seconds=time.monotonic()-start)
        save()
        if result.returncode:
            raise RuntimeError(f'{label} failed; see retained logs')
        return (out / 'logs' / (label + '.stdout')).read_text()

    save()
    try:
        for name in ['clang++', 'llvm-ar', 'llvm-config', 'ld.lld']:
            tool = base / 'bin' / name
            receipt['tools'][name] = {'path': str(tool), 'resolved_path': str(tool.resolve()),
                                       'sha256': sha(tool), 'bytes': tool.stat().st_size}
        config = base / 'bin/llvm-config'
        assert run('version', [config, '--version']).strip() == '23.1.4'
        assert run('rtti', [config, '--has-rtti']).strip() == 'NO'
        assert run('assertions', [config, '--assertion-mode']).strip() == 'OFF'
        cxxflags = [flag for flag in shlex.split(run('cxxflags', [config, '--cxxflags']))
                    if not flag.startswith('-I')]
        for name in component_names:
            original = base / name
            actual = sha(original)
            assert actual == installed['component_inputs'][name], 'baseline component identity changed'
            destination = out / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(original, destination)  # Independent bytes; never a hard link.
            assert sha(destination) == actual
            receipt['component_inputs'][name] = actual
        scalar = out / 'lib/libLLVMScalarOpts.a'
        before = ar_members(scalar)
        member = 'JumpThreading.cpp.o'
        assert member in before and before[member]['magic'].startswith('7f454c460201')
        cpp = out / 'source/JumpThreading.cpp'
        shutil.copy2(sources / args.arm / 'JumpThreading.cpp', cpp)
        obj = out / 'objects' / member
        compile_command = [base / 'bin/clang++', '-I' + str(headers / 'include'), *cxxflags,
                           '-O3', '-DNDEBUG', '-fPIC', '-c', cpp, '-o', obj]
        run('compile-jump-threading', compile_command)
        assert obj.read_bytes()[:6] == b'\x7fELF\x02\x01'
        receipt['replacement_object'] = {'member': member, 'original': before[member],
                                          'source_sha256': sha(cpp), 'sha256': sha(obj),
                                          'bytes': obj.stat().st_size}
        run('replace-member', [base / 'bin/llvm-ar', 'rcsD', scalar, obj])
        after = ar_members(scalar)
        assert set(before) == set(after), 'archive membership changed'
        assert after[member]['sha256'] == sha(obj)
        assert all(before[name] == after[name] for name in before if name != member)
        receipt['archive_member_control'] = {
            'unchanged_members': len(before) - 1, 'only_replaced_member': member,
            'before': before, 'after': after, 'modified_archive_sha256': sha(scalar)}
        # Replay the exact successful installer link recipe, changing isolated
        # input/output paths and adding an explicit maximum linker thread count.
        original_link = next(c['argv'] for c in installed['commands'] if c['name'] == 'link-dso')
        version_map = out / 'logs/llvm.map'
        version_map.write_text('LLVM_23.1 { global: *; };\n')
        old_map = next(x.split('=', 1)[1] for x in original_link if x.startswith('-Wl,--version-script='))
        assert sha(version_map) == sha(Path(old_map))
        library = out / 'lib/libLLVM.so.23.1'
        link = []
        mapped_components = []
        for token in original_link:
            if token in [str(base / name) for name in component_names]:
                name = str(Path(token).relative_to(base))
                token = str(out / name)
                mapped_components.append(name)
            elif token.startswith('-Wl,--version-script='):
                token = '-Wl,--version-script=' + str(version_map)
            elif token == str(base / installed['library']['path']):
                token = str(library)
            link.append(token)
        assert set(mapped_components) == set(component_names) and len(mapped_components) == len(component_names)
        link.insert(1, '-Wl,--threads=' + str(args.jobs))
        assert link[link.index('-o') + 1] == str(library)
        run('link-dso', link)
        (out / 'lib/libLLVM-23.so').symlink_to('libLLVM.so.23.1')
        dynamic = run('dynamic-section', ['readelf', '-d', library])
        assert 'TEXTREL' not in dynamic and 'libLLVM.so.23.1' in dynamic
        notes = run('build-id', ['readelf', '-n', library])
        build_id = re.search(r'Build ID: ([0-9a-f]+)', notes)
        assert build_id
        dependencies = run('dependencies', ['ldd', library])
        assert 'not found' not in dependencies
        loaded = ctypes.CDLL(str(library))
        values = [ctypes.c_uint() for _ in range(3)]
        loaded.LLVMGetVersion.argtypes = [ctypes.POINTER(ctypes.c_uint)] * 3
        loaded.LLVMGetVersion.restype = None
        loaded.LLVMGetVersion(*(ctypes.byref(v) for v in values))
        assert tuple(v.value for v in values) == (23, 1, 4)
        receipt['library'] = {'path': str(library), 'sha256': sha(library),
                              'build_id': build_id.group(1), 'bytes': library.stat().st_size,
                              'version': '23.1.4', 'soname': 'libLLVM.so.23.1'}
        paths = set(re.findall(r'=>\s+(/\S+)\s+\(', dependencies))
        paths.update(re.findall(r'^\s*(/\S+)\s+\(', dependencies, re.M))
        receipt['runtime_dependencies'] = {str(Path(p).resolve()): sha(Path(p)) for p in sorted(paths)}
        receipt['runtime_selection'] = {'LD_LIBRARY_PATH_prepend': str(out / 'lib'),
                                        'LD_PRELOAD': None, 'LD_AUDIT': None,
                                        'requires_fresh_process': True}
        receipt['status'] = 'Pass'
    except Exception as error:
        receipt['status'] = 'Fail'
        receipt['error'] = repr(error)
        raise
    finally:
        save()
        print(json.dumps({'status': receipt['status'], 'receipt': str(receipt_path),
                          'arm': args.arm, 'error': receipt.get('error')}), flush=True)


if __name__ == '__main__':
    main()
