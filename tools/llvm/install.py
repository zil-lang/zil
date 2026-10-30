#!/usr/bin/env python3
"""Install the exact Linux X64 SDK and derive the X86 LLVM DSO in a new prefix.

The official release contains PIC static components, not libLLVM.so. Relinking
follows llvm-shlib's whole-archive/version-map recipe. This is a locally linked
DSO, never labelled an upstream prebuilt shared library. No system installation.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def install(prefix, archive, release, receipt):
    logs = prefix / 'installation-logs'
    logs.mkdir()

    def run(name, argv, env=None):
        record = {'name': name, 'argv': [str(x) for x in argv]}
        receipt['commands'].append(record)
        with (logs / (name + '.stdout')).open('xb') as out, (logs / (name + '.stderr')).open('xb') as err:
            proc = subprocess.run(record['argv'], stdout=out, stderr=err, env=env, timeout=900)
        record['exit_code'] = proc.returncode
        for stream in ('stdout', 'stderr'):
            path = logs / (name + '.' + stream)
            record[stream] = {'path': str(path.relative_to(prefix)), 'sha256': sha(path)}
        if proc.returncode:
            raise RuntimeError(f'{name} failed ({proc.returncode}); see {logs}')
        return (logs / (name + '.stdout')).read_text()

    actual = sha(archive)
    receipt['archive'] = {'bytes': archive.stat().st_size, 'sha256': actual}
    if actual != release['archive_sha256'] or archive.stat().st_size != release['archive_bytes']:
        raise ValueError('official SDK archive size or SHA256 mismatch; nothing extracted')
    # The upstream zstd frame needs a 1 GiB window, beyond zstd's default limit.
    compression = 'zstd --memory=2048MB'
    listing = run('archive-list', ['tar', '-I', compression, '-tf', archive]).splitlines()
    root = release['archive_root']
    names = set(x.rstrip('/') for x in listing)
    tools = ['clang', 'clang-23', 'clang++', 'opt', 'llvm-config', 'llvm-ar', 'lld', 'ld.lld', 'lld-link']
    selected = [root + '/bin/' + name for name in tools]
    selected += [root + '/lib/clang/23', root + '/lib/cmake/llvm']
    selected += sorted(x for x in names if re.fullmatch(re.escape(root) + r'/lib/libLLVM[^/]*\.a', x))
    if any(name not in names for name in selected) or not selected[-1].endswith('.a'):
        raise ValueError('official SDK lacks a required component')
    run('extract', ['tar', '-I', compression, '-xf', archive, '--no-same-owner', '--no-same-permissions',
                    '--strip-components=1', '-C', prefix, '--', *selected])
    config = (prefix / 'lib/cmake/llvm/LLVMConfig.cmake').read_text()
    if not re.search(r'set\(LLVM_ENABLE_PIC\s+ON\)', config):
        raise ValueError('SDK does not declare PIC component archives')
    env = os.environ.copy()
    env['PATH'] = str(prefix / 'bin') + os.pathsep + env.get('PATH', '')
    llvm_config = prefix / 'bin/llvm-config'
    version = run('llvm-version', [llvm_config, '--version'], env).strip()
    if version != release['version']:
        raise ValueError(f'wrong SDK version: {version}')
    libraries = shlex.split(run('component-libraries', [llvm_config, '--link-static', '--libfiles', *release['components']], env))
    libraries = list(dict.fromkeys(libraries))
    if not libraries or any(Path(p).parent != prefix / 'lib' or not Path(p).name.startswith('libLLVM') or
                            not p.endswith('.a') or 'TableGen' in Path(p).name for p in libraries):
        raise ValueError('unexpected LLVM component library selection')
    receipt['component_inputs'] = {str(Path(p).relative_to(prefix)): sha(Path(p)) for p in libraries}
    system = shlex.split(run('system-libraries', [llvm_config, '--link-static', '--system-libs', *release['components']], env))
    receipt['llvm_config_system_libraries'] = system
    # Ubuntu runtime SONAMEs avoid unpinned development archives/symlinks. These
    # are platform dependencies, distinct from the pinned LLVM SDK component set.
    system = ['-l:libz.so.1' if x == '-lz' else
              '-l:libzstd.so.1' if Path(x).name == 'libzstd.a' or x == '-lzstd' else
              '-l:libxml2.so.2' if x == '-lxml2' or Path(x).name.startswith('libxml2.so') else x for x in system]
    receipt['linked_system_libraries'] = system
    major, minor, _ = version.split('.')
    soname = f'libLLVM.so.{major}.{minor}'
    version_map = logs / 'llvm.map'
    version_map.write_text(f'LLVM_{major}.{minor} {{ global: *; }};\n')
    library = prefix / 'lib' / soname
    run('link-dso', [prefix / 'bin/clang++', '-shared', '-fuse-ld=lld', '-Wl,-z,defs', '-Wl,-z,text',
                    '-Wl,-Bsymbolic-functions', '-Wl,--build-id=sha1', '-Wl,-soname,' + soname,
                    '-Wl,--version-script=' + str(version_map), '-Wl,--whole-archive', *libraries,
                    '-Wl,--no-whole-archive', *system, '-o', library], env)
    (prefix / 'lib' / f'libLLVM-{major}.so').symlink_to(soname)
    dynamic = run('dynamic-section', ['readelf', '-d', library], env)
    if 'TEXTREL' in dynamic or soname not in dynamic:
        raise ValueError('derived DSO has text relocations or wrong SONAME')
    loaded = ctypes.CDLL(str(library))
    values = [ctypes.c_uint() for _ in range(3)]
    loaded.LLVMGetVersion.argtypes = [ctypes.POINTER(ctypes.c_uint)] * 3
    loaded.LLVMGetVersion.restype = None
    loaded.LLVMGetVersion(*(ctypes.byref(v) for v in values))
    if '.'.join(str(v.value) for v in values) != version:
        raise ValueError('derived DSO reports the wrong LLVM version')
    dependencies = run('dependencies', ['ldd', library], env)
    if 'not found' in dependencies:
        raise ValueError('derived DSO dependency missing')
    receipt['runtime_dependencies'] = {}
    paths = re.findall(r'=>\s+(/\S+)\s+\(', dependencies)
    paths += re.findall(r'^\s*(/\S+)\s+\(', dependencies, re.M)
    for item in sorted(set(paths)):
        path = Path(item).resolve(strict=True)
        receipt['runtime_dependencies'][str(path)] = sha(path)
    receipt['library'] = {'path': str(library.relative_to(prefix)), 'sha256': sha(library), 'version': version}
    receipt['tools'] = {}
    for name in ('clang', 'opt', 'llvm-config', 'lld-link'):
        banner = run(name + '-version', [prefix / 'bin' / name, '--version'], env)
        if not re.search(r'(?<![0-9.])' + re.escape(version) + r'(?![0-9.])', banner):
            raise ValueError(f'{name} does not report {version}')
        receipt['tools'][name] = {'sha256': sha((prefix / 'bin' / name).resolve()), 'banner': banner.strip()}
    libdir = shlex.quote(str(prefix / 'lib'))
    bindir = shlex.quote(str(prefix / 'bin'))
    (prefix / 'env.sh').write_text(f'export ZIL_LLVM_LIB_DIR={libdir}\nexport ZIL_CLANG={bindir}/clang\n'
                                 f'export LD_LIBRARY_PATH={libdir}${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\n'
                                 f'export PATH={bindir}:$PATH\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', required=True, type=Path, help='new absolute installation directory')
    parser.add_argument('--archive', type=Path, help='reuse a local official archive; always verify its full hash')
    args = parser.parse_args()
    if platform.system() != 'Linux' or platform.machine() not in ('x86_64', 'amd64'):
        parser.error('this installer supports Linux x86_64 hosts only')
    if not args.prefix.is_absolute() or args.prefix.exists():
        parser.error('--prefix must be an absolute directory that does not exist')
    prefix = args.prefix.resolve()
    release = json.loads((HERE / 'release-linux-x86_64.json').read_text())
    if release['version'] != (HERE / 'version.txt').read_text().strip():
        parser.error('release manifest disagrees with version.txt')
    prefix.mkdir(parents=True, exist_ok=False)
    receipt = {'status': 'Fail', 'release': release, 'commands': [], 'installer_sha256': sha(Path(__file__))}
    try:
        with tempfile.TemporaryDirectory(prefix='zil-llvm-download-', dir=prefix.parent) as temporary:
            archive = args.archive.resolve(strict=True) if args.archive else Path(temporary) / 'llvm.tar.zst'
            if args.archive is None:
                subprocess.run(['curl', '--fail', '--location', '--retry', '3', '--connect-timeout', '15',
                                '--max-time', '900', '--output', str(archive), release['url']], check=True)
            install(prefix, archive, release, receipt)
        receipt['status'] = 'Pass'
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        receipt['error'] = str(error)
    (prefix / 'install-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({'status': receipt['status'], 'receipt': str(prefix / 'install-receipt.json'),
                      'error': receipt.get('error')}))
    return 0 if receipt['status'] == 'Pass' else 1


if __name__ == '__main__':
    raise SystemExit(main())
