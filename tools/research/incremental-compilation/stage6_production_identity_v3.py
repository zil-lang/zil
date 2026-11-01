#!/usr/bin/env python3
"""Strict v3 identity: complete closures and opt-in O2 checked scalar leaves v3."""
import hashlib
import json
from pathlib import Path
import re
import shlex
import subprocess

from stage6_toolchain_runtime import clean_environment, command_run, identity, loader_audit, save

PIPELINE = 'default<O2>|merge-functions=after|aliases=false'
NAMESPACE = 'ZIL-OBJECT-ACTION-212-v3-complete-call-closures-checked-scalar-v3'
MACHINE_TRANSFORM = {'name': 'checked-scalar-leaves-v3', 'optimization': 'O2', 'requires_incremental_o2': True,
                     'preflight': 'nonempty-plan-complete-original-model-before-mutation'}
RUST_COMPILER_SHA = 'b53b780f77f952062f8ca8a3b70aef2390b1e45ad91939be94afdc9543026853'
RUST_COMMIT = '58cf8f9c35b355199727627d07ae34a4e2846d67'


def checked_identity(record, base):
    if not isinstance(record, dict) or not all(key in record for key in ['path', 'sha256', 'bytes']):
        raise ValueError('identity requires path, sha256 and bytes')
    path = Path(record['path'])
    if not path.is_absolute():
        path = base / path
    actual = identity(path)
    if actual['sha256'] != record['sha256'] or actual['bytes'] != record['bytes']:
        raise ValueError('changed identity: ' + str(path))
    return actual


def same_identity(record, actual, base):
    if checked_identity(record, base) != actual:
        raise ValueError('receipt identity does not match its production binding')


def load_rust_reference(path):
    path = Path(path).resolve(strict=True)
    record = json.loads(path.read_text())
    if record.get('schema') != 1 or record.get('status') != 'Pass':
        raise ValueError('Rust reference did not pass')
    compiler = checked_identity(record['compiler'], path.parent)
    version = checked_identity(record['version_output'], path.parent)
    output = Path(version['path']).read_text()
    process = record['process']
    if (compiler['sha256'] != RUST_COMPILER_SHA or not output.startswith('rustc 1.99.1 ') or
            '\ncommit-hash: ' + RUST_COMMIT + '\n' not in output or
            '\nLLVM version: 23.1.1\n' not in output or
            process.get('command') != [compiler['path'], '-vV'] or
            process.get('exit') != 0 or process.get('timeout') or process.get('launch_error') is not None or
            Path(process['stdout_path']).resolve() != Path(version['path']) or
            Path(process['stderr_path']).read_bytes()):
        raise ValueError('Rust compiler/version evidence differs from the frozen cross-window reference')
    return {'compiler': compiler, 'version_output': version, '_reference_identity': identity(path)}


def create_rust_reference(compiler, out):
    out = Path(out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    compiler = identity(compiler)
    if compiler['sha256'] != RUST_COMPILER_SHA:
        raise ValueError('not the frozen Rust compiler')
    process = command_run([compiler['path'], '-vV'], out / 'rustc-version', clean_environment(), 30)
    record = {'schema': 1, 'status': 'Pass' if process['exit'] == 0 and not process['timeout'] else 'Fail',
              'compiler': compiler, 'process': process, 'version_output': identity(process['stdout_path']),
              'producer': identity(__file__)}
    save(out / 'RUST-REFERENCE.json', record)
    return load_rust_reference(out / 'RUST-REFERENCE.json')


def load_production_manifest(path):
    """Verify the author's production BUILD receipt and return one shared profile.

    The BUILD receipt is also the source receipt: it contains the exact Git
    revision, complete selected source inventory and pre/post-build checks.
    No second manually asserted source or private research tag is synthesized.
    """
    path = Path(path).resolve(strict=True)
    raw = json.loads(path.read_text())
    source = raw.get('source', {})
    revision = source.get('revision', '')
    if (raw.get('schema') != 1 or raw.get('status') != 'Pass' or
            not re.fullmatch(r'[0-9a-f]{40}', revision) or raw.get('pipeline') != PIPELINE or
            raw.get('action_protocol') != NAMESPACE or raw.get('machine_transform') != MACHINE_TRANSFORM):
        raise ValueError('expected a passing exact production revision, strategy and canonical O2 pipeline')
    compiler = checked_identity(raw['compiler'], path.parent)
    clang = checked_identity(raw['clang'], path.parent)
    library = checked_identity(raw['llvm']['library'], path.parent)
    llvm_receipt = checked_identity(raw['llvm']['receipt'], path.parent)
    receipt_path = Path(llvm_receipt['path'])
    llvm = json.loads(receipt_path.read_text())
    if (llvm.get('status') != 'Pass' or llvm.get('arm') != 'entry-ssa-v4-lvi-cfg' or
            raw['llvm'].get('arm') != llvm['arm']):
        raise ValueError('expected the frozen composed four-TU toolchain receipt')
    same_identity(llvm['library'], library, receipt_path.parent)
    if llvm['library'].get('version') != '23.1.4' or not llvm['library'].get('build_id'):
        raise ValueError('unexpected LLVM version or missing build identity')
    if (source.get('source_matches_revision') is not True or
            source.get('source_matches_revision_after_build') is not True):
        raise ValueError('source receipt does not establish identical sources before and after the build')
    source_root = Path(source['root']).resolve(strict=True)
    inventory = checked_identity(source['manifest'], path.parent)
    files = json.loads(Path(inventory['path']).read_text())
    if not isinstance(files, list) or not files:
        raise ValueError('production source inventory must be a nonempty list')
    scopes = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'tools/llvm/version.txt']
    listed = subprocess.run(['git', '-C', str(source_root), 'ls-tree', '-rz', revision, '--', *scopes],
                            capture_output=True, check=True).stdout
    expected = {}
    for entry in listed.split(b'\0'):
        if not entry:
            continue
        metadata, name = entry.split(b'\t', 1)
        mode, kind, oid = metadata.decode().split()
        if kind != 'blob':
            raise ValueError('non-blob source tree entry')
        expected[name.decode()] = (mode, oid)
    actual_files = {}
    for file in files:
        name = file['path']
        if name in actual_files or name not in expected or (file['mode'], file['git_oid']) != expected[name]:
            raise ValueError('source inventory is duplicated or does not match the exact Git revision')
        actual = checked_identity(file, source_root)
        data = Path(actual['path']).read_bytes()
        git_oid = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
        if git_oid != file['git_oid']:
            raise ValueError('source bytes do not match their Git blob')
        actual_files[name] = actual
    if set(actual_files) != set(expected):
        raise ValueError('production source inventory omits selected Git source files')
    strategy_source = actual_files['crates/zil-backend-llvm/src/object_cache.rs']
    declaration = re.search(r'^const ACTION_PROTOCOL: &str = \"([^\"]+)\";$',
                            Path(strategy_source['path']).read_text(), re.MULTILINE)
    if declaration is None or declaration.group(1) != NAMESPACE:
        raise ValueError('exact v3 action protocol is absent from its bound source')
    hook_source = actual_files['crates/zil-backend-llvm/src/build.rs']
    if not re.search(r'if options\.optimization == Optimization::O2 && options\.incremental_o2\s*\{\s*'
                     r'crate::machine_scalar::fold_known_leaf_calls\(&mut machine, lir\);\s*\}',
                     Path(hook_source['path']).read_text()):
        raise ValueError('bound scalar evaluator must require O2 and incremental_o2')
    process = raw['build']
    if process.get('exit') != 0 or process.get('timeout') is not False or process.get('launch_error') is not None:
        raise ValueError('production compiler build did not complete successfully')
    command = ['cargo', 'build', '--release', '--locked', '--offline', '-p', 'zil-cli', '--features', 'llvm-api']
    environment = process.get('environment_overrides', {})
    target = Path(environment.get('CARGO_TARGET_DIR', '/nonexistent')).resolve()
    library_dir = str(Path(library['path']).parent)
    expected_environment = {'CARGO_TARGET_DIR': str(target), 'CARGO_BUILD_JOBS': '2',
        'ZIL_LLVM_LIB_DIR': library_dir, 'LD_LIBRARY_PATH': library_dir,
        'RUSTFLAGS': '-C link-arg=-Wl,-rpath,' + library_dir, 'ZIL_CLANG': clang['path']}
    if (process.get('command') != command or process.get('cwd') != str(source_root) or
            environment != expected_environment or str(target / 'release/zil') != compiler['path'] or
            process.get('stdout_path') != str(path.parent / 'cargo.stdout') or
            process.get('stderr_path') != str(path.parent / 'cargo.stderr')):
        raise ValueError('build command/source directory/target/compiler/library environment is inconsistent')
    persistent = json.loads((path.parent / 'cargo.process.json').read_text())
    expected_persistent = dict(process)
    expected_persistent.pop('cwd')
    expected_persistent['environment_overrides'] = {name: environment[name] for name in
                                                    ['LD_LIBRARY_PATH', 'ZIL_CLANG', 'ZIL_LLVM_LIB_DIR']}
    if persistent != expected_persistent:
        raise ValueError('BUILD process data does not match the retained actual cargo process record')
    producer = checked_identity(raw['producer'], path.parent)
    helper = checked_identity(raw['helper'], path.parent)
    if helper['sha256'] != 'cc611392b17269b7c6a9bbbc72ac785735a30326d295e230792a8f5d90d3b75c':
        raise ValueError('production build used another process-recording helper')
    if NAMESPACE.encode() not in Path(compiler['path']).read_bytes():
        raise ValueError('compiler ELF lacks its new production action namespace')
    probe = raw['loader_probe']
    if (probe['command'] != [compiler['path'], '--help'] or probe['exit'] != 0 or probe['timeout'] or
            probe['launch_error'] is not None or
            json.loads((path.parent / 'loader.process.json').read_text()) != probe):
        raise ValueError('production help probe does not identify this compiler ELF')
    loaded = {str(Path(line.split('calling init:', 1)[1].strip()).resolve())
              for line in Path(probe['stderr_path']).read_text().splitlines()
              if re.match(r'\s*' + str(probe['pid']) + r':', line) and 'calling init:' in line and 'libLLVM' in line}
    if loaded != {library['path']} or raw['actual_loader_paths'] != [library['path']]:
        raise ValueError('production compiler-PID help probe loaded another LLVM library')
    manifest = identity(path)
    return {'schema': 1, 'status': 'Pass', 'revision': revision, 'compiler': compiler,
            'source_receipt': manifest, 'build_receipt': manifest, 'clang': clang,
            'llvm': {'library': library, 'receipt': llvm_receipt,
                     'version': llvm['library']['version'], 'build_id': llvm['library']['build_id'], 'arm': llvm['arm']},
            'strategy': {'namespace': NAMESPACE, 'source_identity': strategy_source,
                         'flags': ['--incremental-o2'], 'max_new_clone_cost_ratio': 1,
                         'machine_transform': {**MACHINE_TRANSFORM,
                             'source_identity': actual_files['crates/zil-backend-llvm/src/machine_scalar.rs'],
                             'hook_identity': hook_source}},
            'expected_pipeline': PIPELINE, '_manifest_identity': manifest,
            '_source_inventory_identity': inventory, '_verified_source_files': len(actual_files),
            '_build_evidence': {'producer': producer, 'helper': helper,
                'cargo_process': identity(path.parent / 'cargo.process.json'),
                'cargo_stdout': identity(path.parent / 'cargo.stdout'),
                'cargo_stderr': identity(path.parent / 'cargo.stderr'),
                'loader_process': identity(path.parent / 'loader.process.json'),
                'loader_stdout': identity(path.parent / 'loader.stdout'),
                'loader_stderr': identity(path.parent / 'loader.stderr')}}


def production_environment(profile):
    environment = clean_environment()
    environment.update(LD_LIBRARY_PATH=str(Path(profile['llvm']['library']['path']).parent),
                       ZIL_CLANG=profile['clang']['path'])
    return environment


def launcher_text(profile):
    return ('#!/bin/sh\nexport LD_LIBRARY_PATH=' +
            shlex.quote(str(Path(profile['llvm']['library']['path']).parent)) +
            '\nexport ZIL_CLANG=' + shlex.quote(profile['clang']['path']) +
            '\nexec ' + shlex.quote(profile['compiler']['path']) + ' "$@"\n')


def validate_binding(profile, path):
    """Recheck the exact launcher and original compiler-PID loader records."""
    path = Path(path).resolve(strict=True)
    record = json.loads(path.read_text())
    if record.get('status') != 'Pass' or record.get('profile') != profile['_manifest_identity']:
        raise ValueError('launcher binding belongs to a different production profile')
    launcher = checked_identity(record['launcher'], path.parent)
    if (Path(launcher['path']) != path.parent / 'zil-production' or
            Path(launcher['path']).read_text() != launcher_text(profile)):
        raise ValueError('launcher does not exactly select the bound compiler and library')
    checked_identity(record['producer'], path.parent)
    process = record['process']
    environment = production_environment(profile)
    environment.update(LD_DEBUG='libs', LD_DEBUG_OUTPUT=str(path.parent / 'loader'))
    expected_environment = {name: environment[name] for name in
                            ['LD_LIBRARY_PATH', 'ZIL_CLANG', 'LD_DEBUG', 'LD_DEBUG_OUTPUT']}
    if (process.get('command') != [profile['compiler']['path'], '--help'] or
            process.get('exit') != 0 or process.get('timeout') or process.get('launch_error') is not None or
            process.get('environment_overrides') != expected_environment or
            json.loads((path.parent / 'help.process.json').read_text()) != process):
        raise ValueError('binding does not contain the actual successful compiler-PID help probe')
    audit = loader_audit(path.parent / 'loader', process, {'llvm': profile['llvm']})
    if not audit['valid'] or audit != record['loader_audit']:
        raise ValueError('actual compiler-PID loader records changed or identify another LLVM DSO')
    return record


def create_binding(profile, out):
    """Create a launcher and real compiler-PID loader receipt outside measurement."""
    verified = load_production_manifest(profile['_manifest_identity']['path'])
    if verified != profile:
        raise ValueError('production profile changed before launcher binding')
    out = Path(out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    record = {'status': 'Running', 'scope': 'Production identity and help-process loader probe; no language or performance claim',
              'profile': profile['_manifest_identity'], 'producer': identity(__file__)}
    save(out / 'binding.json', record)
    try:
        environment = production_environment(profile)
        environment.update(LD_DEBUG='libs', LD_DEBUG_OUTPUT=str(out / 'loader'))
        process = command_run([profile['compiler']['path'], '--help'], out / 'help', environment, 30)
        audit = loader_audit(out / 'loader', process, {'llvm': profile['llvm']})
        record.update(process=process, loader_audit=audit)
        if process['exit'] != 0 or process['timeout'] or not audit['valid']:
            raise ValueError('actual production compiler PID did not load its bound LLVM library')
        launcher = out / 'zil-production'
        launcher.write_text(launcher_text(profile))
        launcher.chmod(0o755)
        record.update(status='Pass', launcher=identity(launcher))
    except BaseException as error:
        record.update(status='Fail', error=repr(error))
        raise
    finally:
        save(out / 'binding.json', record)
    validate_binding(profile, out / 'binding.json')
    return record
