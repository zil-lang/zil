#!/usr/bin/env python3
"""Strict v5 identity: exact selected current-input O2 source, policy and actual compiler binding."""
import hashlib
import json
from pathlib import Path
import re
import shlex
import subprocess

from stage6_toolchain_runtime import clean_environment, command_run, identity, loader_audit, save

PIPELINE = 'default<O2>|merge-functions=after|aliases=false'
# Selected source policy; BUILD/SOURCE/ELF are generated only from a committed tree.
NAMESPACE = 'ZIL-OBJECT-ACTION-212-v5-current-input-o2'
MACHINE_TRANSFORM = {
    'name': 'checked-chain-v4-then-joint-scalar-summary-v1',
    'optimization': 'O2',
    'requires_incremental_o2': True,
    'ordered_passes': ['checked-add-chain-v4', 'checked-scalar-leaves-v3-with-zeroarg-summary-v1'],
    'preflight': 'each-nonempty-plan-complete-original-model-before-mutation',
    'summary_plan': 'joint-immutable-leaf-replacement-view-one-shared-preflight',
}
DIVISOR_POLICY = {
    'name': 'direct-safe-constant-divisors-v1',
    'requires_incremental_o2': True,
    'max_candidates_per_function': 16384,
    'max_extra_block_instruction_visits': 1000000,
    'fact_source': 'direct typed Constant definitions after exact current VerifiedModule/LIR binding',
    'fallback': 'all original function divisor guards',
    'diagnostic_environment': 'none',
}
STABLE_IDENTITY_POLICY = {
    'name': 'closed-filesystem-flat-scalar-stable-identity-v1',
    'selected_as': 'necessary-bookkeeping-in-closed-dce-bundle-not-independent-performance-promotion',
    'driver_gate': 'native-Build-default-entry-Linux-LLVM-API-O2-incremental-closed-units2..64',
    'provenance': 'current-filesystem-canonical-root-and-relative-module-length-framed-declaration-spelling',
    'binding': 'complete-current-request-function-bijection-after-Analyze-and-Verify',
    'max_syntax_items': 65536,
    'max_declarations': 10000,
    'max_modules': 10000,
    'max_signature_entries': 100000,
    'max_identity_bytes': 8388608,
    'max_key_bytes': 16384,
    'fallback': 'original-pipeline-without-stable-hints',
}
CLOSED_DCE_POLICY = {
    'name': 'closed-flat-entry-reachability-dce-v2',
    'gate': 'closed-Linux-LLVM-API-O2-incremental-Main-units>=2-current-bound-hints',
    'domain': 'flat-scalar-no-source-definition-calls-except-native-wrapper-to-source-entry',
    'original_owner_occupancy_minimum': 2,
    'max_functions': 10000,
    'max_key_bytes': 8388608,
    'max_work': 1000000,
    'preflight': 'complete-unchanged-original-model-before-prune-rename-or-sort',
    'retained': 'native-wrapper-source-entry-and-their-required-declarations',
    'ordering': 'prune-then-rename-then-explicit-partition-then-closed-internalization',
    'fallback': 'unchanged-original-source-DSU-complete-closure-partition',
    'includes': 'pruning-and-required-stable-identity-bookkeeping',
}
SELECTED_POLICIES = {
    'source_transforms': MACHINE_TRANSFORM,
    'machine_preflight_mode': {
        'name': 'validate-only-reachable-schedule-elision-v1',
        'scope': 'all-current-validate_module-callers-including-O0-nonoptin-and-cache-hits',
        'requires_incremental_o2': False,
    },
    'native_schedule_mode': 'unchanged-collect-schedule',
    'machine_partition_strategy': {
        'general_graph_selected': False,
        'general_partition': 'existing-source-owner-whole-dsu-groups',
        'closed_partition': 'prepared-stable-partition-only-after-strict-closed-dce-admission',
    },
    'stable_identity_strategy': STABLE_IDENTITY_POLICY,
    'closed_reachability_pruning': CLOSED_DCE_POLICY,
    'machine_lowering_policy': DIVISOR_POLICY,
}
DIAGNOSTIC_NAMES = ('ZIL_RESEARCH_GRAPH_DIAGNOSTIC', 'ZIL_RESEARCH_GRAPH_MODEL',
                    'ZIL_RESEARCH_CLOSED_STABLE_TRACE', 'ZIL_RESEARCH_CLOSED_STABLE_DCE_TRACE')
# Literal actual formatted source records; independent review binds these exact bytes.
SOURCE_ROLE_PINS = {'crates/zil-backend-llvm/src/build.rs': ('c96f560074ac812a868878c9893c331b1595ce9530a9712d5f2250a8e76b4dda',
                                          32207),
 'crates/zil-backend-llvm/src/emit.rs': ('89de0b5e922bcd2da1e26e51b0114e1e0ae011b8e4da7062e574d17750b770ae',
                                         112083),
 'crates/zil-backend-llvm/src/emit/constant_divisors.rs': ('74dff591f0e6628515094be42eb5dd868113868e6017ae8cc7f478146498303a',
                                                           2100),
 'crates/zil-backend-llvm/src/lib.rs': ('f203b591f83f01b34c35aa8760c4cdf80d7b42cfb946dd5ecdc7107d31ba3292',
                                        2563),
 'crates/zil-backend-llvm/src/machine_cgu.rs': ('c59685bf416b7058732bb021af06b1fcbfa7db14e4f4e17db40558ba464a2a09',
                                                50353),
 'crates/zil-backend-llvm/src/machine_checked_add_chain.rs': ('1842aa8266312a82859185d928219aa7be0fbd347199bb1bdfa8cb1574ebcba7',
                                                              19792),
 'crates/zil-backend-llvm/src/machine_scalar.rs': ('ac9fb2d89bb399ebaa604e7339fc5c81643c53c512f4de7162102ed96ae7e5d3',
                                                   22103),
 'crates/zil-backend-llvm/src/machine_scalar_summary.rs': ('c83db70710584f2539f3ce5f8971196fd08063d9aa4d8c299de421a121b81bed',
                                                           16949),
 'crates/zil-backend-llvm/src/machine_stable_identity.rs': ('784178d28e0ecc9765821b9325d4e377ca27f9250b09ce6807f04f72af7d742d',
                                                            21970),
 'crates/zil-backend-llvm/src/object_cache.rs': ('7fac6b4934400d11fbfaa376dc1d4875bdb09cf891c77fbbffc76b3a30e9eb53',
                                                 23644),
 'crates/zil-driver/src/lib.rs': ('58ad2fb5a0f3b912d441997eca6ca36cd0642d82798de18568e2dc8153d070d4', 11474),
 'crates/zil-driver/src/project.rs': ('b5f977d2e8b610e4b9afdeddd1e67fcd30705b9c814b781236551bfff7b70a3d',
                                      48314),
 'crates/zil-driver/src/session.rs': ('b01f7e0c02f60863db35e977987e5cfee0899cd6c4d1c5f4e94f3c17b37fb33d',
                                      55539),
 'crates/zil-driver/src/stable_source_identity.rs': ('760c2a34075ca7fcdf5ee05c4591e9459ff15e57fb61e9c9158085da257a2bf1',
                                                     23517),
 'crates/zil-llvm-api/src/lib.rs': ('dbfb60522e056038d9b592b8056a952d14f42926f5653944bf9f84ffb29017bb', 6526),
 'crates/zil-llvm-api/src/native.rs': ('2883489929e0617bc808ee3683af0c99caf24f55d1aead80ca11fbf564c1508d',
                                       16770),
 'crates/zil-llvm-api/src/preflight.rs': ('90750a50aba306aebabd967b6dd5e5117855d3164e95ed1ccdcada46c95016bc',
                                          44294)}


def require_selected_contract():
    if (NAMESPACE != 'ZIL-OBJECT-ACTION-212-v5-current-input-o2' or len(SOURCE_ROLE_PINS) != 17 or
            any(not isinstance(value, tuple) or len(value) != 2 or
                not re.fullmatch(r'[0-9a-f]{64}', value[0]) or
                not isinstance(value[1], int) or value[1] <= 0 for value in SOURCE_ROLE_PINS.values())):
        raise ValueError('production v5 selected source contract is incomplete')


def sanitized_environment(environment):
    result = dict(environment)
    for name in DIAGNOSTIC_NAMES:
        result.pop(name, None)
    return result


def diagnostic_off_record(environment):
    if any(name in environment for name in DIAGNOSTIC_NAMES):
        raise ValueError('research diagnostic environment reached production')
    return {name: None for name in DIAGNOSTIC_NAMES}


def validate_selected_source(actual_files):
    """Bind every selected implementation and caller to actual reviewed source bytes."""
    require_selected_contract()
    roles = {}
    texts = {}
    for relative, expected in SOURCE_ROLE_PINS.items():
        actual = actual_files[relative]
        if (actual['sha256'], actual['bytes']) != expected or identity(actual['path']) != actual:
            raise ValueError('selected compiled source differs from reviewed v5 role: ' + relative)
        roles[relative] = actual
        texts[relative] = ' '.join(Path(actual['path']).read_text().split())
    prefix = 'crates/zil-backend-llvm/src/'
    if any(prefix + name in actual_files for name in ['machine_cgu_graph.rs', 'machine_cgu_graph_engine.rs']):
        raise ValueError('unselected graph implementation is present in production source')
    if re.search(r'\bmachine_cgu_graph(?:_engine)?\b', texts[prefix + 'machine_cgu.rs']):
        raise ValueError('unselected graph owner hook is present in production source')
    build = texts[prefix + 'build.rs']
    hooks = [
        'let mut machine = crate::emit::lower_view_with_divisor_policy( verified, lir, &options.emit, options.optimization == Optimization::O2 && options.incremental_o2, )?;',
        'if options.optimization == Optimization::O2 && options.incremental_o2 { crate::machine_checked_add_chain::fold_checked_add_chains(&mut machine); crate::machine_scalar::fold_known_leaf_calls(&mut machine, lir); }',
        'let mut stable = if options.closed_executable && options.backend == NativeBackend::LlvmApi && options.optimization == Optimization::O2 && options.incremental_o2 && options.codegen.units >= 2 && options.emit.entry == EntryPoint::Main && lir.target.triple == "x86_64-unknown-linux-gnu" { stable_names.and_then(|names| { crate::machine_stable_identity::prepare( &machine, lir, names, options.codegen.units, cancelled, ) }) } else { None };',
        'check_cancelled(cancelled)?; if let Some(plan) = &mut stable { plan.prune_unreachable_closed_definitions(&mut machine); plan.rename(&mut machine); }',
        'let llvm = machine.to_text();',
        'let (mut plan, stable_candidates, stable_diagnostic) = if let Some(stable) = stable {',
        'if options.closed_executable { if let Some(candidates) = stable_candidates { crate::machine_cgu::internalize_closed_executable_names(&mut plan.units, &candidates); } else { crate::machine_cgu::internalize_closed_executable(&mut plan.units, lir); } }',
    ]
    position = 0
    for hook in hooks:
        found = build.find(hook, position)
        if found < 0:
            raise ValueError('selected source hook/order/gate differs from v5 contract: ' + hook)
        position = found + len(hook)
    emit = texts[prefix + 'emit.rs']
    for hook in [
        'pub(crate) fn lower_view_with_options( verified: &VerifiedModule, lir: &LirView<\'_>, options: &EmitOptions, ) -> CompileResult<m::Module> { lower_view_with_divisor_policy(verified, lir, options, false) }',
        'pub(crate) fn lower_view_with_divisor_policy( verified: &VerifiedModule, lir: &LirView<\'_>, options: &EmitOptions, constant_divisor_guards: bool, ) -> CompileResult<m::Module> { bind_lir(verified, lir)?;',
        'safe_divisors: constant_divisor_guards.then(BTreeSet::new)',
        'constant_divisors::retain_safe(self.function, &mut self.safe_divisors);',
    ]:
        if hook not in emit:
            raise ValueError('selected current binding/direct divisor lowering contract changed')
    api = texts['crates/zil-llvm-api/src/lib.rs']
    proof = texts['crates/zil-llvm-api/src/preflight.rs']
    native = texts['crates/zil-llvm-api/src/native.rs']
    if ('pub fn validate_module(module: &model::Module) -> Result<(), Error> { preflight::validate_only(module)' not in api or
            'pub(crate) fn validate(module: &Module) -> Result<Schedule> { validate_impl::<true>(module) }' not in proof or
            'pub(crate) fn validate_only(module: &Module) -> Result<()> { validate_impl::<false>(module).map(|_| ()) }' not in proof or
            'let schedule = crate::preflight::validate(program)' not in native):
        raise ValueError('validation-only API/native collecting schedule contract changed')
    driver = texts['crates/zil-driver/src/stable_source_identity.rs']
    if ('options.profile == super::ExecutionProfile::Native && options.stop_after == super::Stage::Build && options.closed_executable && options.target.triple == "x86_64-unknown-linux-gnu" && options.backend == super::NativeBackend::LlvmApi && options.optimization == super::Optimization::O2 && options.incremental_o2 && options.entry.is_none() && (2..=64).contains(&options.codegen.units)' not in driver or
            'let stable_names = stable_source.and_then(|source| source.bind(verified.module()));' not in texts['crates/zil-driver/src/session.rs']):
        raise ValueError('current filesystem identity option gate or request binding changed')
    return {'compiled_source_roles': roles,
            'pipeline_order': ['current-verified-LIR-binding-and-selected-divisor-lowering',
                'checked-add-chain-v4', 'joint-scalar-summary-v1',
                'closed-immutable-admission-and-complete-original-validation',
                'closed-prune-then-rename-if-admitted', 'actual-partition-and-closed-internalization',
                'current-per-unit-validation-and-native-collecting-schedule'],
            'graph_selected': False, 'closed_strategy': CLOSED_DCE_POLICY['name']}


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
    require_selected_contract()
    path = Path(path).resolve(strict=True)
    raw = json.loads(path.read_text())
    source = raw.get('source', {})
    revision = source.get('revision', '')
    if (raw.get('schema') != 1 or raw.get('status') != 'Pass' or
            not re.fullmatch(r'[0-9a-f]{40}', revision) or raw.get('pipeline') != PIPELINE or
            raw.get('action_protocol') != NAMESPACE or raw.get('machine_transform') != MACHINE_TRANSFORM or
            raw.get('selected_policies') != SELECTED_POLICIES or
            raw.get('diagnostic_environment') != {'build': diagnostic_off_record({}), 'help': diagnostic_off_record({})}):
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
        raise ValueError('exact v5 action protocol is absent from its bound source')
    selected_source_contract = validate_selected_source(actual_files)
    if raw.get('selected_source_contract') != selected_source_contract:
        raise ValueError('final selected source hooks differ from production BUILD')
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
    identity_tool = checked_identity(raw['identity_tool'], path.parent)
    local_identity_tool = identity(__file__)
    if (identity_tool['sha256'], identity_tool['bytes']) != (local_identity_tool['sha256'], local_identity_tool['bytes']):
        raise ValueError('production builder used another selected policy/source contract module')
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
                         'machine_transform': MACHINE_TRANSFORM,
                         'selected_policies': SELECTED_POLICIES,
                         'selected_source_contract': selected_source_contract},
            'diagnostic_environment': diagnostic_off_record({}),
            'expected_pipeline': PIPELINE, '_manifest_identity': manifest,
            '_source_inventory_identity': inventory, '_verified_source_files': len(actual_files),
            '_build_evidence': {'producer': producer, 'helper': helper,
                'identity_tool': identity_tool,
                'cargo_process': identity(path.parent / 'cargo.process.json'),
                'cargo_stdout': identity(path.parent / 'cargo.stdout'),
                'cargo_stderr': identity(path.parent / 'cargo.stderr'),
                'loader_process': identity(path.parent / 'loader.process.json'),
                'loader_stdout': identity(path.parent / 'loader.stdout'),
                'loader_stderr': identity(path.parent / 'loader.stderr')}}


def production_environment(profile):
    require_selected_contract()
    environment = sanitized_environment(clean_environment())
    environment.update(LD_LIBRARY_PATH=str(Path(profile['llvm']['library']['path']).parent),
                       ZIL_CLANG=profile['clang']['path'])
    diagnostic_off_record(environment)
    return environment


def launcher_text(profile):
    require_selected_contract()
    return ('#!/bin/sh\nunset ' + ' '.join(DIAGNOSTIC_NAMES) + '\nexport LD_LIBRARY_PATH=' +
            shlex.quote(str(Path(profile['llvm']['library']['path']).parent)) +
            '\nexport ZIL_CLANG=' + shlex.quote(profile['clang']['path']) +
            '\nexec ' + shlex.quote(profile['compiler']['path']) + ' "$@"\n')


def validate_binding(profile, path):
    """Recheck the exact launcher and original compiler-PID loader records."""
    path = Path(path).resolve(strict=True)
    record = json.loads(path.read_text())
    if (record.get('status') != 'Pass' or record.get('profile') != profile['_manifest_identity'] or
            record.get('diagnostic_environment') != diagnostic_off_record({})):
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
              'profile': profile['_manifest_identity'], 'producer': identity(__file__),
              'diagnostic_environment': diagnostic_off_record(production_environment(profile))}
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
