#!/usr/bin/env python3
"""Read-only current push consumer receipt checks; never infer missing work as zero.

CLI accepts one report JSON or resident response JSON. This is instrumentation
validation, not a performance, semantic or independent-authority oracle. The
matched runner separately checks current Verify, diagnostics and native output.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path

# Explicit experiment pins, not evidence of the remote branch's current head.
MAIN = '42f0396f36b2a36860ecb55371d436e4327877a4'
LEGACY_MAIN = '99035931d358add0eaed6d4535035c38413701dd'
BASELINE_PINS = (MAIN, LEGACY_MAIN)
LLVM_RAW = 'complete diagnostic LLVM bytes'
LLVM_PUSH = 'complete diagnostic LLVM bytes except the strict second-line verified-fingerprint hex64 value; raw bytes and hashes retained'
CANDIDATE_FLAGS = frozenset(('--producer-reuse', '--reactive-queries', '--push-reuse'))
FIELDS = {
    'raw_inference_work': ('full_runs', 'sparse_batches', 'replay_fallbacks',
                           'closure_bodies_checked', 'closure_bodies_reused',
                           'name_functions_checked', 'name_functions_reused',
                           'dependency_bodies_checked', 'dependency_bodies_reused'),
    'types_push_work': ('context_rebuilds', 'sparse_batches', 'body_checks',
                        'position_rebindings', 'retained_functions', 'receipt_tree_updates'),
    'initialization': ('product_admission_functions_checked', 'product_function_queries_executed',
                       'product_function_queries_reused', 'product_global_obligations_executed',
                       'product_global_obligations_reused', 'product_plan_rows_replaced',
                       'product_semantic_functions_compared', 'product_relocated_function_bindings'),
    'verification_work': ('full_receipts_reused', 'function_proofs_checked', 'function_proofs_reused',
                          'instruction_proofs_reused', 'empty_borrow_proofs_checked',
                          'empty_borrow_proofs_reused', 'empty_borrow_values_avoided',
                          'raw_shape_functions_checked', 'raw_shape_functions_reused',
                          'raw_inventory_functions_checked', 'raw_inventory_functions_reused',
                          'raw_shape_functions_aggregated', 'raw_shape_ranges_reused',
                          'raw_result_modes_checked', 'raw_result_modes_reused',
                          'resource_inventory_functions_checked', 'resource_inventory_functions_reused',
                          'resource_inventory_ranges_reused', 'foreign_functions_selected',
                          'foreign_functions_not_visited', 'environment_functions_selected',
                          'environment_functions_not_visited', 'region_functions_checked',
                          'region_functions_reused', 'lexical_cleanup_functions_checked',
                          'lexical_cleanup_functions_reused', 'closure_instance_functions_checked',
                          'closure_instance_functions_reused', 'closure_instance_ranges_reused', 'raw_function_headers_reused'),
    'lowering_work': ('functions_checked', 'layout_checks', 'functions_reused', 'full_checks',
                      'budget_replays', 'context_checks', 'context_check_ns'),
}


NATIVE_PRODUCT_FIELDS = (
    'available', 'optimizer_function_stage_executions', 'optimizer_function_stage_reuses',
    'optimizer_module_pass_executions', 'optimizer_cgscc_pass_executions',
    'analysis_reconstructions', 'ir_key_bytes', 'ir_snapshot_bytes', 'ir_replay_bytes',
    'machine_function_executions', 'machine_function_reuses', 'machine_pass_skips',
    'asm_printer_runs', 'mir_key_bytes', 'mir_snapshot_bytes', 'mir_replay_bytes',
    'mir_key_ns', 'mir_parse_ns', 'mir_restore_validate_ns', 'mir_capture_ns',
    'retained_bytes', 'failed_replays', 'fresh_context_fallbacks', 'fallback_ns',
)
NATIVE_WORK_SCOPE = (
    'native_products hook counters and MIR scope timers describe the first hooked attempt only; '
    'after fresh_context_fallbacks, subsequent ordinary native work has unavailable per-pass counts. '
    'Reuse in a discarded attempt is not work avoided by final compilation. '
    'Native receipt phase/total times and request wall include the full native fallback; '
    'request wall also includes all driver retries. MIR timers are scopes, not a partition of native total.'
)


def inspect_native_products(report):
    native = report.get('native_codegen')
    if native is None:
        return None
    if not isinstance(native, dict) or not isinstance(native.get('object_reports'), list):
        raise ValueError('native_codegen.object_reports must retain actual unit receipts')
    result = []
    for index, unit in enumerate(native['object_reports']):
        unit = json.loads(unit) if isinstance(unit, str) else unit
        if not isinstance(unit, dict):
            raise ValueError('native object receipt must be an object')
        stats = unit.get('native_products')
        if stats is not None:
            if not isinstance(stats, dict):
                raise ValueError('native_products must be an object or null')
            for field in NATIVE_PRODUCT_FIELDS:
                if type(stats.get(field)) is not int or stats[field] < 0:
                    raise ValueError('native_products.' + field + ' missing or not a nonnegative integer')
        result.append({'unit_index': index, 'stats': stats,
                       'fresh_context_fallbacks': None if stats is None else stats['fresh_context_fallbacks']})
    return {'units': result, 'scope': NATIVE_WORK_SCOPE, 'unavailable_is_not_zero': True}


def candidate_flags(flags):
    if (not isinstance(flags, list) or any(not isinstance(flag, str) for flag in flags)
            or not set(flags) <= CANDIDATE_FLAGS or len(flags) != len(set(flags))):
        raise ValueError('candidate extras must be unique research switches; no profile/backend/cache/budget overrides')
    return flags


def baseline_pin(actual, requested):
    if requested not in BASELINE_PINS or actual != requested:
        raise ValueError('baseline source differs from the explicit supported experiment pin')


def zil_command(*, binary, mode, source, work, arm, extras):
    """The complete formal command, including its declared artifact locations."""
    if mode not in ('check', 'o2') or arm not in ('baseline', 'candidate'):
        raise ValueError('unsupported formal Zil command mode or arm')
    extras = candidate_flags(extras)
    paths = []
    for value in (binary, source, work):
        if not isinstance(value, (str, Path)):
            raise ValueError('formal command paths must be absolute')
        path = Path(value)
        if not path.is_absolute() or '..' in path.parts or '\0' in str(path):
            raise ValueError('formal command paths must be absolute and have no parent traversal')
        # Do not resolve against the validating machine: a recovered archive
        # retains the original command's paths without requiring them to exist.
        paths.append(path)
    binary, source, work = paths
    command = [str(binary), 'check' if mode == 'check' else 'build', str(source),
               '--json', '--resident', '--resident-capacity', '256',
               '--resident-reset-after', '4096', '--semantic-cache', str(work / 'semantic')]
    if mode == 'o2':
        command += ['--backend', 'llvm-api', '--opt', 'O2', '--codegen-units', '16',
                    '--codegen-jobs', '1', '--object-cache', str(work / 'objects'),
                    '-o', str(work / 'program')]
    if arm == 'candidate':
        command += extras
    return command


def command_flags(command, arm, extras, *, binary, mode, source, work):
    """Reject every undeclared token, duplicate/override and retargeted path."""
    expected = zil_command(binary=binary, mode=mode, source=source, work=work,
                           arm=arm, extras=extras)
    if (not isinstance(command, list) or any(type(arg) is not str for arg in command)
            or command != expected):
        raise ValueError('complete formal Zil argv differs from declared protocol for ' + arm)


def rust_command(*, cargo, mode, target):
    if mode not in ('check', 'o2') or not isinstance(cargo, str) or not cargo:
        raise ValueError('unsupported formal Cargo command')
    target = Path(target)
    if not target.is_absolute() or '..' in target.parts:
        raise ValueError('Cargo target must be absolute without parent traversal')
    command = [cargo, 'check' if mode == 'check' else 'build', '--offline', '--locked',
               '--jobs', '1', '--verbose', '--message-format=json', '--target-dir', str(target)]
    if mode == 'o2':
        command.append('--release')
    return command


def inspect_commands(run, manifest):
    """Bind all recorded argv to the original manifest, also after relocation."""
    original = Path(manifest['arguments']['out'])
    if not original.is_absolute() or '..' in original.parts:
        raise ValueError('original run location must be absolute without parent traversal')
    extras = candidate_flags(manifest['arguments'].get('candidate_extra_arg', []))
    for block in range(6):
        recorded_block = Path(run) / f'block-{block:02d}'
        original_block = original / f'block-{block:02d}'
        for arm in ('baseline', 'candidate'):
            command = json.loads((recorded_block / arm / 'command.json').read_text())['argv']
            command_flags(command, arm, extras, binary=manifest['binaries'][arm]['path'],
                          mode=manifest['arguments']['mode'],
                          source=original_block / 'source' / 'main.zil', work=original_block / arm)


def inspect_runtime_environments(manifest):
    """Validate declared paths without consulting the original host after restore."""
    if manifest.get('runtime_binding_schema') != 'per-arm-resident-llvm-v1':
        raise ValueError('explicit per-arm resident LLVM binding is missing')
    environments = manifest['arm_runtime_environments']
    for arm in ('baseline', 'candidate'):
        env = environments[arm]
        path = env['LD_LIBRARY_PATH']
        if (type(path) is not str or not Path(path).is_absolute() or '..' in Path(path).parts
                or ':' in path or env['ZIL_LLVM_LIB_DIR'] != path
                or manifest['arguments'][arm + '_llvm_library_path'] != path
                or env.get('LD_PRELOAD') or env.get('LD_AUDIT')):
            raise ValueError('arm LLVM environment differs from its explicit declared directory')
    return environments


def inspect_runtime_receipt(receipt, environment, pid, out, check_blob, parse_mappings):
    """Bind archived mapped files to kernel identities and the declared arm env."""
    if (receipt['schema'] != 'resident-llvm-maps-v1' or receipt['environment'] != environment
            or type(pid) is not int or pid <= 0 or receipt['pid'] != pid):
        raise ValueError('resident LLVM receipt PID/environment binding differs')
    check_blob(receipt['maps'])
    mappings = parse_mappings((Path(out) / receipt['maps']['archive']).read_bytes())
    dsos = receipt['llvm_dsos']
    identities = [{key: item[key] for key in ('path', 'device', 'inode')} for item in dsos]
    if not mappings or identities != mappings:
        raise ValueError('archived LLVM DSO list differs from actual mapped files')
    directory = Path(environment['LD_LIBRARY_PATH'])
    for item in dsos:
        resolved = Path(item['resolved_path'])
        stamp = item['file_stamp']
        if (not resolved.is_absolute() or '..' in resolved.parts or not resolved.is_relative_to(directory)
                or len(stamp) != 5 or any(type(value) is not int or value < 0 for value in stamp)
                or [os.major(stamp[0]), os.minor(stamp[0])] != item['device']
                or stamp[1] != item['inode'] or stamp[2] != item['bytes']):
            raise ValueError('LLVM DSO namespace/device/inode/size binding differs')
        check_blob(item)
    return dsos


def llvm_comparison_sha256(data):
    """Only the fixed second-line identity value differs between proof domains."""
    prefix = re.match(rb'\A; ZIL verified LLVM module\n; verified-fingerprint: ([0-9a-f]{64})\n', data)
    if prefix is None:
        raise ValueError('LLVM diagnostic lacks the exact second-line verified-fingerprint hex64 header')
    start, end = prefix.span(1)
    return hashlib.sha256(data[:start] + b'0' * 64 + data[end:]).hexdigest()


def inspect_report(report, *, mode, cold=False, profiled=False):
    """Validate available receipts and require them when current stages ran.

    This validator is for the fixed closed scalar push workload. Successful
    current Ct requires raw inference stats; whole Ct hits need no new receipt.
    Complete frontend/candidate hits legitimately lack Types/Analyze receipts.
    Failed requests can omit raw inference stats, so None is unknown,
    not zero. Events and execution assertions remain separate from field shape.
    Cold means the *first* request of a fresh block, never a later cold preparation.
    """
    if mode not in ('check', 'o2'):
        raise ValueError('unsupported push measurement mode')
    events = report['query_events']
    executed = {event['phase'] for event in events if event['disposition'] == 'executed'}
    front = report.get('frontend_stats')
    if front is None:
        front = {}
    if not isinstance(front, dict):
        raise ValueError('frontend_stats must be an object or null')
    groups = {name: (front.get(name) if name == 'initialization' else report.get(name)) for name in FIELDS}
    present = {name: name in (front if name == 'initialization' else report) for name in FIELDS}
    for name, fields in FIELDS.items():
        value = groups[name]
        if value is None:
            continue
        if not isinstance(value, dict):
            raise ValueError(name + ' must be an object or null')
        for field in fields:
            number = value.get(field)
            if type(number) is not int or number < 0:
                raise ValueError(name + '.' + field + ' missing or not a nonnegative integer')
        if name == 'lowering_work' and value.get('cost_profiled') is not profiled:
            raise ValueError('lowering_work.cost_profiled differs from the declared timing mode')
    failed_types = report['status'] != 'Success' and report['phase'] == 'types'
    if type(report['verification_runs']) is not int or report['verification_runs'] < 0:
        raise ValueError('verification_runs must be a nonnegative integer')
    required = {
        'raw_inference_work': report['status'] == 'Success' and 'ct' in executed,
        'types_push_work': 'types' in executed and not failed_types,
        'initialization': 'analyze' in executed,
        'verification_work': report['verification_runs'] > 0,
        'lowering_work': 'lower' in executed or report['phase'] == 'lower'
                         or (mode == 'o2' and report['status'] == 'Success'),
    }
    for name, needed in required.items():
        if needed and groups[name] is None:
            raise ValueError(name + ' unavailable despite current stage execution')
    if report['status'] == 'Success':
        if report['verification_runs'] != 1 or sum(event == {'phase': 'verify', 'disposition': 'executed'} for event in events) != 1:
            raise ValueError('successful push request lacks one current independent Verify invocation')
        if mode == 'o2' and 'lower' not in executed:
            raise ValueError('successful native request lacks current Lower execution')
    if cold:
        if report['status'] != 'Success' or not {'types', 'analyze', 'verify'} <= executed:
            raise ValueError('first cold request did not execute current pipeline stages')
        if groups['types_push_work']['body_checks'] <= 0:
            raise ValueError('first cold request did not check any bodies')
        analysis = groups['initialization']
        if analysis['product_function_queries_executed'] + analysis['product_global_obligations_executed'] <= 0:
            raise ValueError('first cold request did not execute actual Analyze consumers')
        verify = groups['verification_work']
        if verify['raw_shape_functions_checked'] + verify['raw_shape_functions_aggregated'] <= 0:
            raise ValueError('first cold request did not perform independent raw shape checks')
        if mode == 'o2' and groups['lowering_work']['functions_checked'] <= 0:
            raise ValueError('first cold request did not admit any Lower functions')
    return {'groups': groups, 'fields_present': present, 'required_by_execution': required,
            'native_products': inspect_native_products(report), 'unavailable_is_not_zero': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--mode', choices=('check', 'o2'), required=True)
    parser.add_argument('--cold', action='store_true', help='only the first request in a fresh block')
    parser.add_argument('--profiled', action='store_true', help='diagnostic window only, never formal wall comparison')
    args = parser.parse_args()
    try:
        value = json.loads(args.report.read_text())
        result = inspect_report(value.get('report', value), mode=args.mode, cold=args.cold, profiled=args.profiled)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps({'valid': False, 'error': str(error)}))
        return 1
    print(json.dumps({'valid': True, **result}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
