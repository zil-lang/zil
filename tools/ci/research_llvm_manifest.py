#!/usr/bin/env python3
"""Bind research LLVM CI to its actual DSO and preserve bounded build evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


VARIANT = 'entry-ssa-v4-lvi-cfg'
MEMBERS = {'JumpThreading.cpp.o', 'SSAUpdaterBulk.cpp.o',
           'LazyValueInfo.cpp.o', 'PassBuilderPipelines.cpp.o'}


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def identity(path):
    path = path.resolve(strict=True)
    return {'path': str(path), 'sha256': sha(path), 'bytes': path.stat().st_size}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def document(path):
    value = json.loads(path.read_text())
    require(isinstance(value, dict), 'Expected an object: ' + str(path))
    return value


def bound_file(record, expected):
    actual = identity(expected)
    require(Path(record['path']).resolve(strict=True) == expected.resolve(strict=True),
            'Recorded path does not match expected file: ' + str(expected))
    require(record['sha256'] == actual['sha256'], 'SHA mismatch: ' + str(expected))
    return actual


def verify(repo, temporary):
    output = temporary / 'receipts/llvm-research-identity.json'
    output.parent.mkdir(parents=True, exist_ok=True)
    report = {'status': 'Fail', 'scope': __doc__, 'verifier': identity(Path(__file__))}
    try:
        require(not os.environ.get('LD_PRELOAD') and not os.environ.get('LD_AUDIT'),
                'Loader interposition is not allowed')
        llvm = temporary / 'llvm-23.1.4'
        headers = temporary / 'llvm-headers'
        research = temporary / 'llvm-research'
        candidate = research / 'candidate'
        pin = document(repo / 'tools/llvm/release-linux-x86_64.json')
        installed = document(llvm / 'install-receipt.json')
        header = document(headers / 'headers-receipt.json')
        rebuilt = document(research / 'rebuild-receipt.json')
        native = document(candidate / 'native-toolchain-receipt.json')
        checked = document(temporary / 'receipts/llvm-api-toolchain.json')
        source = document(research / 'source/source-manifest.json')
        revision = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
        report['revision'] = revision
        for name, value in [('install', installed), ('headers', header), ('rebuild', rebuilt),
                            ('candidate', native), ('check_toolchain', checked)]:
            require(value.get('status') == 'Pass', name + ' did not pass')
        require(pin['version'] == '23.1.4' and installed['release'] == pin,
                'Official installation does not match repository SDK pin')
        for name, value in [('install', installed), ('headers', header)]:
            require(value['archive']['sha256'] == pin['archive_sha256']
                    and value['archive']['bytes'] == pin['archive_bytes'], name + ' SDK mismatch')
        require(header['release_manifest_sha256'] == sha(repo / 'tools/llvm/release-linux-x86_64.json'),
                'Header release manifest mismatch')
        require(rebuilt['revision'] == revision and rebuilt['max_jobs'] == 2,
                'Rebuild checkout/jobs mismatch')
        bound_file(rebuilt['producer'], repo / 'tools/research/incremental-compilation/toolchain_combination/rebuild.py')
        require(all(c.get('exit') == 0 for c in rebuilt['commands']), 'Failed rebuild command')
        require(all(c.get('returncode') == 0 for c in native['commands']), 'Failed native build command')
        require(native['arm'] == VARIANT and source['variant'] == VARIANT,
                'Unexpected research candidate variant')
        require(source['upstream']['commit'] == '7a941dfb2877a0b8cf30323fefa85ad019e0b0f5'
                and source['public_headers_unchanged'] is True, 'Unexpected LLVM source/header boundary')
        require(native['base_install_receipt_sha256'] == sha(llvm / 'install-receipt.json')
                and native['headers_receipt_sha256'] == sha(headers / 'headers-receipt.json'),
                'Candidate base/header receipt mismatch')
        report['candidate_receipt'] = bound_file(rebuilt['toolchains']['candidate'], candidate / 'native-toolchain-receipt.json')
        report['original_receipt'] = bound_file(rebuilt['toolchains']['original'], research / 'original/native-toolchain-receipt.json')
        bound_file(native['recipe'], research / 'source/candidate-recipe.json')
        recipe = document(research / 'source/candidate-recipe.json')
        require(recipe == native['recipe_contents'] and recipe['arm'] == VARIANT,
                'Candidate recipe differs from its build receipt')
        require(len(recipe['replacements']) == 4
                and {x['member'] for x in recipe['replacements']} == MEMBERS,
                'Expected exactly the four reviewed replacement units')
        require(set(source['files']) == {arm + '/' + member[:-2] for arm in ('original', 'candidate') for member in MEMBERS},
                'Expected the complete original/candidate source file set')
        report['source_files'] = {}
        for name, digest in source['files'].items():
            path = research / 'source' / name
            require(path.resolve(strict=True).is_relative_to((research / 'source').resolve()),
                    'Source manifest path escapes generated source directory')
            require(sha(path) == digest, 'Generated source mismatch: ' + name)
            report['source_files'][name] = identity(path)
        for replacement in recipe['replacements']:
            bound_file(replacement['source'], research / 'source/candidate' / replacement['member'][:-2])
            bound_file(replacement['source_manifest'], research / 'source/source-manifest.json')
        report['patch'] = bound_file(source['patch'], research / 'source/combined.patch')
        library = candidate / 'lib/libLLVM.so.23.1'
        report['library'] = bound_file(native['library'], library)
        require(native['library']['version'] == '23.1.4'
                and native['library']['soname'] == 'libLLVM.so.23.1', 'Candidate library version/SONAME mismatch')
        bound_file(checked['checks']['llvm'], library)
        require(checked['checks']['llvm']['version'] == '23.1.4', 'Loaded LLVM version mismatch')
        require(Path(os.environ['ZIL_LLVM_LIB_DIR']).resolve() == library.parent.resolve()
                and os.environ.get('LD_LIBRARY_PATH') == str(library.parent.resolve()),
                'Tests must select only the candidate library directory')
        for name, check_name in [('clang', 'clang'), ('lld-link', 'lld_link')]:
            actual = bound_file(checked['checks'][check_name], llvm / 'bin' / name)
            require(actual['sha256'] == installed['tools'][name]['sha256'],
                    'Official executable changed: ' + name)
        require(Path(os.environ['ZIL_CLANG']).resolve() == (llvm / 'bin/clang').resolve(),
                'ZIL_CLANG must remain the official SDK executable')
        report['toolchain_check'] = identity(temporary / 'receipts/llvm-api-toolchain.json')
        report['status'] = 'Pass'
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        report['error'] = str(error)
        raise
    finally:
        output.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'status': report['status'], 'receipt': str(output)}))
    return report


def collect(temporary):
    output = temporary / 'receipts/llvm-research-evidence'
    output.mkdir(parents=True, exist_ok=False)
    record = {'status': 'Collecting', 'scope': 'Generated source/patch/log/receipt evidence only; no SDK, DSO, archive, object or Cargo target upload',
              'files': {}, 'missing_roots': [], 'excluded': [], 'collector': identity(Path(__file__))}
    roots = [temporary / 'llvm-research', temporary / 'llvm-23.1.4/installation-logs']
    exact = [temporary / 'llvm-23.1.4/install-receipt.json',
             temporary / 'llvm-headers/headers-receipt.json', temporary / 'llvm-headers/download.log',
             temporary / 'llvm-headers/extract.log']
    allowed = {'.cpp', '.h', '.patch', '.json', '.stdout', '.stderr', '.log', '.map', '.md', '.txt'}

    def copy(path):
        require(not path.is_symlink() and path.resolve(strict=True).is_relative_to(temporary),
                'Evidence path is not a regular contained file: ' + str(path))
        relative = path.relative_to(temporary)
        destination = output / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, destination)
        require(sha(path) == sha(destination), 'Evidence copy mismatch: ' + str(path))
        record['files'][str(relative)] = {'bytes': destination.stat().st_size, 'sha256': sha(destination)}

    try:
        for root in roots:
            if not root.exists():
                record['missing_roots'].append(str(root.relative_to(temporary)))
                continue
            for current, directories, filenames in os.walk(root, followlinks=False):
                for name in list(directories):
                    path = Path(current) / name
                    if name in ('lib', 'objects', 'target', 'include', 'bin') or name.endswith('-target') or path.is_symlink():
                        directories.remove(name)
                        record['excluded'].append(str(path.relative_to(temporary)))
                for name in filenames:
                    path = Path(current) / name
                    if path.is_symlink() or path.suffix.lower() not in allowed:
                        record['excluded'].append(str(path.relative_to(temporary)))
                    else:
                        copy(path)
        for path in exact:
            if path.exists():
                copy(path)
            else:
                record['missing_roots'].append(str(path.relative_to(temporary)))
        record['status'] = 'Collected'
    except (OSError, ValueError) as error:
        record['status'] = 'Fail'
        record['error'] = str(error)
        raise
    finally:
        (output / 'MANIFEST.json').write_text(json.dumps(record, indent=2) + '\n')
        print(json.dumps({'status': record['status'], 'output': str(output), 'files': len(record['files'])}))
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['verify', 'collect'])
    parser.add_argument('--runner-temp', type=Path, required=True)
    parser.add_argument('--repo', type=Path, default=Path.cwd())
    args = parser.parse_args()
    temporary = args.runner_temp.resolve(strict=True)
    try:
        if args.action == 'verify':
            verify(args.repo.resolve(strict=True), temporary)
        else:
            collect(temporary)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()
