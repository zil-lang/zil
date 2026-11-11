"""Draft native-prelink harness support. Import never runs compiler/native tools."""
import hashlib
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent / 'vendor'))
from processes import measured
from runtime_gate import stats
from runtime_fixture import generate_runtime
from fixture import generate

ROOT = Path(__file__).resolve().parent
BASELINE_COMMIT = '99035931d358add0eaed6d4535035c38413701dd'
BASELINE_TREE = '1c16744d838064cb1c687ba56ef3a0d17ed3130c'
LLVM_DSO_SHA = '2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250'
LLVM_VERSIONS = {'zil_native': '23.1.4', 'clang': '23.1.4', 'rustc': '1.99.1', 'rustc_bundled_llvm': '23.1.1'}
REFERENCE_SHA = 'a30ffe86586c77849a613be636ece4f58fa3bfcebbc0eec3191cba333ce4e72c'
ORDERS = ['ABR', 'BRA', 'RAB', 'ARB', 'RBA', 'BAR']
COUNTS = {'cases': 2, 'formal_compilations': 36, 'native_total': 108,
          'native_formal': 36, 'native_functional': 72}
CONTROLLED_ENV = ['ZIL_RESEARCH_THINLTO_RECEIPT', 'ZIL_RESEARCH_THINLTO_COMPLETE',
                  'ZIL_THINLTO_PROTOCOL', 'ZIL_THINLTO_PROTOCOL_SHA', 'ZIL_THINLTO_LOGDIR',
                  'ZIL_PRIVATE_IMPORTS_MANIFEST', 'ZIL_PRIVATE_IMPORTS_MANIFEST_SHA256',
                  'ZIL_PRIVATE_IMPORTS_REQUEST_ID', 'ZIL_PRIVATE_IMPORTS_RECEIPT_DIR',
                  'ZIL_RESEARCH_PRELINK_RECEIPT']


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def identity(path):
    path = Path(os.path.abspath(path))
    path.resolve(strict=True)
    return {'path': str(path), 'sha256': sha(path)}


def dso_identity(path):
    path = Path(os.path.abspath(path))
    return {'path': str(path), 'resolved_path': str(path.resolve(strict=True)), 'sha256': sha(path)}


def scripts():
    return {str(path.relative_to(ROOT)): sha(path) for path in sorted(ROOT.rglob('*.py'))}


def clear_research():
    for name in CONTROLLED_ENV:
        os.environ.pop(name, None)


def validate(protocol):
    if protocol['format'] != 'zil-full-producer-compile-screen/1' or protocol['status'] != 'prepared-not-run':
        raise ValueError('unknown native-prelink protocol')
    if protocol['scripts'] != scripts():
        raise ValueError('harness source changed')
    if set(protocol['tools']) != {'baseline', 'candidate', 'rustc', 'clang'}:
        raise ValueError('native harness must not introduce old wrapper/plugin tools')
    if (protocol['llvm_dso']['sha256'] != LLVM_DSO_SHA
            or dso_identity(protocol['llvm_dso']['path']) != protocol['llvm_dso']
            or protocol['dynamic_linker_environment'] != {'LD_LIBRARY_PATH': os.environ.get('LD_LIBRARY_PATH')}
            or protocol['llvm_versions'] != LLVM_VERSIONS):
        raise ValueError('actual native LLVM DSO/path/loader environment changed')
    # Symmetric process-external prevalidation. The measured compiler request
    # does every actual Verify/model/LLVM/prelink/postlink check itself.
    for value in protocol['tools'].values():
        if identity(value['path']) != value:
            raise ValueError('frozen tool invocation/path/bytes changed')
    if set(protocol['build_receipts']) != {'A', 'B'}:
        raise ValueError('both compiler build bindings required')
    for arm, value in protocol['build_receipts'].items():
        if identity(value['path']) != value:
            raise ValueError('build binding changed')
        binding = json.loads(Path(value['path']).read_text())
        tool = protocol['tools']['baseline' if arm == 'A' else 'candidate']
        claim = protocol['sources'][arm]
        if (binding.get('source_commit') != claim['commit']
                or binding.get('source_tree') != claim['tree']
                or binding.get('binary_sha256') != tool['sha256']
                or str(Path(binding['binary']).resolve()) != tool['path']
                or binding.get('release_build_status') != 'Pass'
                or sha(binding['build_log']) != binding['build_log_sha256']):
            raise ValueError('compiler source/tree/binary/build-log binding incomplete')
    if protocol['sources']['A'] != {'commit': BASELINE_COMMIT, 'tree': BASELINE_TREE}:
        raise ValueError('baseline changed')
    for field in ['commit', 'tree']:
        value = protocol['sources']['B'][field]
        if len(value) != 40 or any(c not in '0123456789abcdef' for c in value):
            raise ValueError('candidate exact Git identities required')
    reference = protocol['reference_protocol']
    if identity(reference['path']) != reference or reference['sha256'] != REFERENCE_SHA:
        raise ValueError('original linear reference changed')
    original = json.loads(Path(reference['path']).read_text())
    if protocol['cases'] != original['cases']:
        raise ValueError('original case sourcehash/oracle/vectors changed')
    if (protocol['cpus'] != original['cpus'] or protocol['jobs'] != 2
            or protocol['orders'] != ORDERS or protocol['iterations'] != 100000
            or protocol['limits'] != {'B_over_A_max': 1.10, 'R_over_B_min': .90}
            or protocol['compile_screen_counts'] != COUNTS or list(protocol['cases']) != ['n64/base', 'n256/base']):
        raise ValueError('fixed two-case schedule or gates changed')
    if (len(protocol['cpus']) != 2 or len(set(protocol['cpus'])) != 2
            or not set(protocol['cpus']).issubset(os.sched_getaffinity(0))):
        raise ValueError('frozen two-CPU budget unavailable')
    if (protocol['candidate_receipt_schema'] != 'zil-manual-prelink-preflight/1+unit/2+object/2'
            or protocol['identity_validation_policy'] != 'symmetric-process-external-prevalidation; complete-native-compiler-request-timed'):
        raise ValueError('native receipt or identity timing contract changed')


def run(command, log, timeout=600, data=None):
    result = measured([str(v) for v in command], Path(log), timeout, data)
    write(Path(log) / 'raw-metrics.json', result)
    if result['exit'] != 0 or result['timeout'] or result['exception'] or result['residual_descendants']:
        raise RuntimeError('request failed; raw retained: ' + str(log))
    return result


def _names(value, label):
    if not isinstance(value, list) or any(not isinstance(v, str) or not v for v in value) or value != sorted(set(value)):
        raise ValueError('unsorted/duplicate/non-name list: ' + label)
    return set(value)


def _positive(value, label):
    if type(value) is not int or value <= 0:
        raise ValueError('current positive timing/size absent: ' + label)


def validate_candidate_receipt(path, report, fixture, protocol):
    machine = json.loads(Path(path).read_text())
    nc = report['native_codegen']
    if (machine.get('format') != 'zil-manual-prelink-preflight/1'
            or machine.get('backend') != 'llvm-api'
            or machine.get('request') != 'O2+incremental-o2+after'
            or machine.get('actual_units') != 8 or machine.get('cache') != 'disabled'
            or machine.get('summary_index') is not False
            or nc.get('backend') != 'llvm-api' or nc.get('actual_units') != 8
            or nc.get('requested_units') != 8 or nc.get('jobs') != 2 or nc.get('fallback') is not None
            or len(nc.get('object_reports', [])) != 9 or nc['object_reports'][0] != machine):
        raise ValueError('missing current native prelink/ordinary-bitcode pipeline')
    cache = nc['object_cache']
    if cache.get('requested') is not False or cache.get('enabled') is not False or any(cache.get(k) != 0 for k in ['hits', 'misses', 'stored']):
        raise ValueError('this screen requires no object cache')
    for key in ['original_preflight_ns', 'partition_input_preflight_ns']:
        _positive(machine[key], key)
    if [u['unit'] for u in machine['units']] != list(range(8)):
        raise ValueError('incomplete current machine preflight')
    for u in machine['units']:
        _positive(u['preflight_ns'], 'unit preflight')
    proof = machine['visibility_proof']
    if (proof['format'] != 'zil-thinlto-complete-visibility/1'
            or any(proof.get(k) is not True for k in ['current_complete_planner', 'canonical_owners_equal', 'original_required_exactly_allowed'])
            or [u['unit'] for u in proof['units']] != list(range(8))):
        raise ValueError('current typed Complete proof missing')
    owners, allowed, costs = {}, {}, {}
    total_copy = 0
    for u in proof['units']:
        i = u['unit']
        own = _names(u['canonical'], 'canonical')
        if not own or own & owners.keys():
            raise ValueError('canonical owners overlap or empty')
        owners.update({name: i for name in own})
        listed = u['allowed']
        allowed[i] = _names([v['name'] for v in listed], 'Allowed')
        if _names(u['required_original'], 'original Required') != allowed[i]:
            raise ValueError('original Complete closure changed')
        subtotal = 0
        for v in listed:
            _positive(v['cost'], 'original source function cost')
            if v['name'] in costs and costs[v['name']] != v['cost']:
                raise ValueError('conflicting original source cost')
            costs[v['name']] = v['cost']
            subtotal += v['cost']
        if subtotal != u['copy_cost']:
            raise ValueError('original unit budget mismatch')
        total_copy += subtotal
    _positive(proof['original_source_cost'], 'source cost')
    if total_copy != proof['total_copy_cost'] or total_copy > proof['original_source_cost']:
        raise ValueError('original total copy budget exceeded')
    for i, names in allowed.items():
        if any(n not in owners or owners[n] == i or n == 'main' for n in names):
            raise ValueError('Allowed has no cross-unit canonical owner')
    objects = nc['object_reports'][1:]
    graph, intrinsics, prelinks = {}, set(), {}
    target = {'llvm_version': '23.1.4', 'triple': 'x86_64-unknown-linux-gnu', 'cpu': 'x86-64',
              'features': '', 'relocation': 'PIC', 'code_model': 'default'}
    for i, obj in enumerate(objects):
        pre = obj['prelink']
        if (pre.get('format') != 'zil-research-manual-prelink-unit/2' or pre.get('unit') != i
                or pre.get('pipeline') != 'default<O2>' or pre.get('bitcode_kind') != 'ordinary-no-summary'
                or any(pre.get(k) is not True for k in ['verify_before', 'verify_after', 'current_structured_preflight'])
                or any(pre.get(k) != v for k, v in target.items())):
            raise ValueError('actual prelink stage/verification/target missing')
        for key in ['bitcode_bytes', 'preflight_ns', 'builder_ns', 'optimize_ns', 'serialize_ns', 'total_ns']:
            _positive(pre[key], 'prelink ' + key)
        canonical = _names(pre['canonical'], 'prelink canonical')
        definitions = _names(pre['definitions'], 'actual definitions')
        declarations = _names(pre['declarations'], 'actual declarations')
        known_intrinsics = _names(pre['intrinsics'], 'LLVMGetIntrinsicID intrinsic names')
        if (canonical != set(proof['units'][i]['canonical']) or definitions != canonical
                or definitions & declarations or not known_intrinsics <= declarations
                or set(pre['direct_calls']) != definitions):
            raise ValueError('actual prelink canonical/definition/graph mismatch')
        intrinsics |= known_intrinsics
        for function, calls in pre['direct_calls'].items():
            edges = _names(calls, 'actual direct callees')
            if function in graph or not edges <= definitions | declarations:
                raise ValueError('actual LLVM graph duplicate/undeclared target')
            graph[function] = edges
        prelinks[i] = (definitions, declarations)
    foreign = _names(proof['foreign'], 'current foreign')
    if foreign & owners.keys() or not foreign <= {'getchar', 'putchar'} | intrinsics:
        raise ValueError('unknown foreign callable')
    for definitions, declarations in prelinks.values():
        if not declarations <= set(owners) | foreign | intrinsics:
            raise ValueError('unowned/non-intrinsic actual declaration')
    details = []
    pipeline = 'canonical-full<O2>|budgeted-direct-imports=private|empty-required=emit|nonempty-required=thinlto<O2>|merge-functions=after|aliases=false'
    for i, obj in enumerate(objects):
        seen, pending, required = set(), list(proof['units'][i]['canonical']), set()
        while pending:
            name = pending.pop()
            if name in seen:
                continue
            seen.add(name)
            if name not in graph:
                raise ValueError('actual required body missing')
            for callee in graph[name]:
                if callee in owners:
                    pending.append(callee)
                    if owners[callee] != i:
                        required.add(callee)
                elif callee not in foreign | intrinsics:
                    raise ValueError('unknown actual closure target')
        if not required <= allowed[i]:
            raise ValueError('actual Required exceeds original Allowed')
        research = obj['research']
        if (research.get('format') != 'zil-research-manual-prelink-object/2' or research.get('unit') != i
                or research.get('official_thinlto_importer') is not False or research.get('native_cache') is not False
                or research.get('actual_bitcode_graph_checked') is not True
                or research.get('actual_required_path_verified') is not True
                or _names(research['required'], 'actual Required') != required
                or _names(research['allowed'], 'actual Allowed') != allowed[i]
                or research.get('donor_modules') != len({owners[n] for n in required})):
            raise ValueError('actual typed prelink/import/postlink contract absent')
        changes = research['imports']
        if [c['canonical'] for c in changes] != sorted(required):
            raise ValueError('actual import coverage incomplete')
        for c in changes:
            if (c['private'] != f"zil.import.{i}.{c['canonical']}" or c['private'] in owners
                    or type(c['direct_uses']) is not int or c['direct_uses'] <= 0):
                raise ValueError('private direct-only import evidence invalid')
        if (obj.get('backend') != 'llvm-api' or any(obj.get(k) != v for k, v in target.items())
                or obj.get('verify_before_after') is not True or obj.get('pipeline') != pipeline
                or obj.get('input_kind') != 'research-current-prelinked-bitcode' or not obj.get('data_layout')):
            raise ValueError('actual postlink O2/object target evidence absent')
        for key in ['object_bytes', 'parse_ns', 'emit_ns', 'total_ns']:
            _positive(obj[key], 'object ' + key)
        # Only actual transitive Required controls skipping a second O2. The
        # original Allowed budget can remain nonempty on the emit-only path.
        if research.get('postlink_optimization_ran') is not bool(required):
            raise ValueError('postlink branch does not match actual Required')
        if obj['optimize_ns'] != research['postlink_optimize_ns']:
            raise ValueError('object/research actual optimization timing differs')
        if required:
            _positive(obj['optimize_ns'], 'nonempty Required postlink O2')
            _positive(research['postlink_optimize_ns'], 'nonempty Required reported postlink O2')
            _positive(research['trim_link_ns'], 'actual donor trim/link')
        elif (type(obj['optimize_ns']) is not int or obj['optimize_ns'] != 0
                or type(research['postlink_optimize_ns']) is not int or research['postlink_optimize_ns'] != 0
                or type(research['trim_link_ns']) is not int or research['trim_link_ns'] != 0
                or research['donor_modules'] != 0 or changes):
            raise ValueError('empty Required must emit verified full-O2 canonical bytes without donor/second-O2 work')
        details.append({'unit': i, 'canonical': len(proof['units'][i]['canonical']), 'Allowed': len(allowed[i]),
                        'Required': len(required), 'donor_modules': research['donor_modules'],
                        'direct_uses': sum(c['direct_uses'] for c in changes),
                        'current_required_path_verified': True,
                        'postlink_optimization_ran': bool(required)})
    commands = report['commands']
    if (len(commands) != 1 or commands[0][0] != protocol['tools']['clang']['path']
            or '-O2' not in commands[0] or '--target=x86_64-unknown-linux-gnu' not in commands[0]
            or len([v for v in commands[0] if v.endswith('.o')]) != 8
            or any(any(token in v for token in ['flto', 'pass-plugin', 'thinlto', 'opt-remarks']) for v in commands[0])):
        raise ValueError('final link is not the expected ordinary pinned Clang link')
    return {'current_machine': machine, 'objects': objects, 'independent_actual_graph': details,
            'current_typed_Complete_copy_cost': total_copy, 'actual_prelink_units': 8, 'actual_emitted_objects': 8,
            'actual_postlink_optimization_units': sum(bool(v['Required']) for v in details),
            'summary_index': False, 'official_thinlto_importer': False, 'actual_checks_inside_compiler': True}
