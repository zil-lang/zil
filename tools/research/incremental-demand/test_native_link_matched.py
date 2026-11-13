#!/usr/bin/env python3
"""Pure source/controller/publication mocks; never start a compiler or executable."""
import importlib.util
import copy
import json
from pathlib import Path
import struct
import tempfile
import types
import unittest
from unittest.mock import patch


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


runner = load('three_arm_runner', Path(__file__).with_name('native_link_matched_measure.py'))
controller = load('three_arm_controller', Path(__file__).with_name('native_link_matched_controller.py'))
work = load('fixed_workload', Path(__file__).resolve().parents[1]/'reactive-query-graph/native_workloads.py')


class ThreeArmControlTests(unittest.TestCase):
    def args(self, out):
        return types.SimpleNamespace(out=out, mode='o2', candidate_extra_arg=['--push-reuse'],
                                     native_link_profile=Path('/workspace/.zil-environment/native-link-v1/manifest'),
                                     native_link_profile_sha256='f'*64)

    def pins(self):
        item = {'source': 'a'*40, 'binary': '/fixed/zil', 'worktree': '/fixed/source',
                'build_receipt': '/fixed/build.json', 'binary_sha256': 'b'*64,
                'llvm_library_path': '/fixed/lib', 'llvm_dso_sha256': 'c'*64}
        return {'schema': 'native-link-three-arm-pins-v1', 'tool_source': 'd'*40,
                'baseline': dict(item, source=controller.MAIN), 'candidate': item,
                'native_link_profile': '/workspace/.zil-environment/native-link-v1/manifest',
                'native_link_profile_sha256': 'e'*64}

    def test_full_nine_fixtures_and_real_positions_in_both_languages(self):
        self.assertEqual(runner.LABELS, ['cold', 'unchanged', 'private', 'multi', 'broad', 'positions', 'error', 'recovery', 'restore'])
        for label in runner.LABELS:
            zil = work.snapshot('scalar', label, 20, 20, 16)
            rust = runner.rust_files(work, label, 20, 20, 16)
            meta = runner.rc.validate_equivalence(zil, rust, 20, 20, 16, label)
            self.assertEqual((meta['named_declarations'], meta['arithmetic_step_count'], len(zil), len(rust)), (425, 6400, 22, 22))
        cold = runner.rust_files(work, 'cold', 20, 20, 16)
        moved = runner.rust_files(work, 'positions', 20, 20, 16)
        prefix = '// Source position edit; semantic body is unchanged.\n\n'
        self.assertEqual({name: value[len(prefix):] for name, value in moved.items()}, cold)
        self.assertTrue(all(moved[name] != cold[name] for name in cold))
        self.assertEqual([(runner.rc.parent(label), label) for label in runner.LABELS][-3:],
                         [('cold', 'error'), ('error', 'recovery'), ('private', 'restore')])

    def test_controller_has_all_three_arms_and_explicit_profile_exact_pins(self):
        for mode in ('check', 'o2'):
            command = controller.runner_command(self.pins(), mode, Path('/fixed/run'), 2, True, True)
            self.assertIn('--run', command)
            self.assertEqual(command.count('--baseline'), 1)
            self.assertEqual(command.count('--candidate'), 1)
            self.assertEqual(command[command.index('--blocks')+1], '2')
            self.assertIn('--expected-baseline-dso-sha256', command)
            self.assertIn('--expected-candidate-dso-sha256', command)
            self.assertEqual('--publication-recovery' in command, mode == 'o2')
        bad = self.pins()
        bad['baseline']['source'] = 'a'*40
        with self.assertRaises(ValueError):
            controller.validate_pins(bad)
        for flags in ([], ['--profile-compilation'], ['--push-reuse', '--max-nodes', '0'],
                      ['--push-reuse', '--native-link-profile', '/arbitrary']):
            with self.assertRaises(ValueError):
                runner.candidate_flags(flags)

    def test_actual_zil_command_has_profile_only_on_candidate(self):
        args = self.args(Path('/fixed/out'))
        for mode in ('check', 'o2'):
            args.mode = mode
            for arm in ('baseline', 'candidate'):
                command = runner.zil_command(args, '/fixed/zil', '/fixed/source/main.zil', '/fixed/work', arm)
                self.assertEqual('--native-link-profile' in command, arm == 'candidate' and mode == 'o2')
                self.assertEqual('--push-reuse' in command, arm == 'candidate')
                self.assertNotIn('--profile-compilation', command)
                if mode == 'o2':
                    self.assertEqual(command[command.index('--codegen-units')+1], '16')
                    self.assertEqual(command[command.index('--codegen-jobs')+1], '1')

    def test_ir_representation_can_differ_but_whole_elf_oracle_and_units_cannot(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            args = self.args(out)
            args.native_repetitions = 2
            executable = runner.blob(out, b'\x7fELF'+b'complete same machine program'*8, '.elf')
            report = {'status': 'Success', 'phase': 'link', 'diagnostics': [], 'input_fingerprint': 'a'*64,
                      'source_files': {'0': '/fixed/main.zil'}, 'scalar_interfaces': []}
            oracle = {'exit': 0, 'stdout_hex': '1234', 'stderr_hex': ''}
            outcomes = [{'valid': True, 'expected': oracle, 'actual': oracle} for _ in range(12)]
            pair = []
            for i, arm in enumerate(runner.ARMS):
                row = {'arm': arm, 'valid': True, 'native': outcomes, 'executable': executable}
                if arm != 'rust':
                    row.update({'resident': {'report': report},
                                'llvm': {'sha256': str(i)*64, 'comparison_sha256_excluding_identity_comment': str(i+2)*64},
                                'inventory': {'object_count': 16, 'compiler_summary': {'actual_units': 16},
                                              'current_layout_matches': ['e'*64],
                                              'objects': [{'defined_functions': ['f'+str(j)]} for j in range(16)]}})
                pair.append(row)
            result = runner.quality(pair, args)
            self.assertTrue(result['valid'])
            self.assertFalse(result['raw_llvm_equal'])
            self.assertFalse(result['llvm_equal_excluding_identity_comment'])
            self.assertFalse(result['llvm_equality_is_acceptance_prerequisite'])
            pair[1]['executable'] = runner.blob(out, b'\x7fELF'+b'different machine program'*8, '.elf')
            self.assertFalse(runner.quality(pair, args)['valid'])
            pair[1]['executable'] = executable
            pair[2]['native'] = [{'valid': False, 'expected': oracle, 'actual': dict(oracle, exit=1)}]*12
            self.assertFalse(runner.quality(pair, args)['valid'])
            pair[2]['native'] = outcomes
            pair[1]['inventory']['object_count'] = 15
            self.assertFalse(runner.quality(pair, args)['valid'])
            pair[1]['inventory']['object_count'] = 16
            pair[1]['inventory']['objects'][0]['defined_functions'] = ['unexpected-export']
            self.assertFalse(runner.quality(pair, args)['valid'])
            with self.assertRaises(ValueError):
                runner.quality(pair[:2], args)

    def test_typed_stage_observations_keep_unknown_and_reject_malformed_values(self):
        self.assertIsNone(runner.typed_stage_observations({'native_codegen': None}))
        def report(stats):
            return {'native_codegen': {'object_reports': [{'native_products': stats}]}}
        for stats in (None, {}, {'stage_body': None}):
            self.assertIsNone(runner.typed_stage_observations(report(stats))[0]['stage_body'])
        typed = dict.fromkeys(runner.STAGE_BODY_FIELDS, 0)
        typed.update(observer_schema=1, captures=2, captured_bytes=123)
        self.assertEqual(runner.typed_stage_observations(report({'stage_body': typed}))[0]['stage_body'], typed)
        for key, value in [('observer_schema', 2), ('captures', True), ('replays', -1), ('prepare_ns', None)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                runner.typed_stage_observations(report({'stage_body': dict(typed, **{key: value})}))

    def test_compiled_source_and_tools_commit_are_separate_without_allowing_production_drift(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            source, tools = 'a'*40, 'b'*40

            def git(argv, **kwargs):
                command = argv[3:] if argv[1] == '-C' else argv[1:]
                if command == ['rev-parse', 'HEAD']:
                    return (tools+'\n').encode()
                if command == ['diff', '--name-only', source, 'HEAD']:
                    return b'tools/research/incremental-demand/new_tool.py\n'
                if command[:2] == ['status', '--porcelain=v1']:
                    return b''
                if command == ['diff', '--name-only', 'HEAD'] or command[:1] == ['ls-files']:
                    return b''
                if command[:1] == ['ls-tree']:
                    return b'pinned full compiled source inventory\n'
                if command == ['diff', '--binary', 'HEAD']:
                    return b''
                if command == ['rev-parse', source+'^{tree}']:
                    return b'c'*40+b'\n'
                raise AssertionError(command)

            with patch.object(runner.subprocess, 'check_output', side_effect=git), \
                 patch.object(runner.subprocess, 'run', return_value=types.SimpleNamespace(returncode=0)):
                identity = runner.source_identity(out, source, out, 'candidate')
            self.assertEqual(identity['commit'], source)
            self.assertEqual(identity['checkout_commit'], tools)

            def drift(argv, **kwargs):
                if argv[3:] == ['diff', '--name-only', source, 'HEAD']:
                    return b'crates/zil-backend-llvm/src/build.rs\n'
                return git(argv, **kwargs)

            with patch.object(runner.subprocess, 'check_output', side_effect=drift):
                with self.assertRaises(ValueError):
                    runner.source_identity(out, source, out, 'candidate')

    def test_actual_object_exports_must_bind_the_current_physical_layout(self):
        def object_bytes(name):
            header = bytearray(64)
            header[:6] = b'\x7fELF\x02\x01'
            strings = b'\0'+name.encode()+b'\0'
            symbols = bytes(24)+struct.pack('<IBBHQQ', 1, 2, 0, 1, 0, 1)
            section_offset = 64+len(strings)+len(symbols)
            struct.pack_into('<Q', header, 40, section_offset)
            struct.pack_into('<HH', header, 58, 64, 3)
            sections = bytes(64)
            sections += struct.pack('<IIQQQQIIQQ', 0, 3, 0, 0, 64, len(strings), 0, 0, 1, 0)
            sections += struct.pack('<IIQQQQIIQQ', 0, 2, 0, 0, 64+len(strings), len(symbols), 1, 0, 8, 24)
            return bytes(header)+strings+symbols+sections

        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            cache = out/'objects'
            namespace = cache/'objects-v1'/('a'*64)
            namespace.mkdir(parents=True)
            reports = []
            payload = (16).to_bytes(4, 'little')
            for index in range(16):
                key = f'{index+1:064x}'
                data = object_bytes('f'+str(index))
                container = b'ZILOBJ01'+bytes.fromhex(key)+bytes.fromhex(runner.rc.sha(data))+len(data).to_bytes(8, 'little')+data
                (namespace/(key+'.zobj')).write_bytes(container)
                reports.append({'backend': 'llvm-api', 'pipeline': 'default<O2>', 'object_bytes': len(data),
                                'cache': {'state': 'miss', 'action_key': key}})
                payload += index.to_bytes(4, 'little')+bytes(12)+(1).to_bytes(4, 'little')+index.to_bytes(4, 'little')
            layouts = cache/'partition-layout-v1'
            layouts.mkdir()
            data = b'ZILCGU01'+bytes.fromhex('b'*64)+bytes.fromhex(runner.rc.sha(payload))+payload
            (layouts/('b'*64+'.layout')).write_bytes(data)
            report = {'native_codegen': {'backend': 'llvm-api', 'requested_units': 16, 'actual_units': 16,
                                        'jobs': 1, 'fallback': None, 'object_reports': reports,
                                        'object_cache': {'hits': 0, 'misses': 16, 'corrupt': 0, 'write_failures': 0, 'stored': 16}}}
            llvm = b'; ZIL verified LLVM module\n; verified-fingerprint: '+b'a'*64+b'\n'+b''.join(('define void @f'+str(i)+'() {\n ret void\n}\n').encode() for i in range(16))

            def symbols(args, item):
                blob = runner.blob(args.out, b'physical symbol evidence\n'+item['sha256'].encode(), '.nm')
                return {'path': blob['archive'], 'sha256': blob['sha256']}

            args = self.args(out)
            with patch.object(runner, 'symbols', symbols):
                inventory = runner.zil_inventory(args, report, cache, llvm, 'c'*64)
                self.assertEqual(inventory['object_count'], 16)
                self.assertEqual(inventory['current_layout_matches'], ['b'*64])
                artifact = {'reused_program': True, 'retained_files': True, 'before_codegen': True,
                            'current_preflight_checked_units': 0, 'current_preflight_reused_units': 16,
                            'profile_id': 'f'*64, 'llvm_verification_scope': 'retained-producer',
                            'current_llvm_verifier_executed': False}
                retained = {'backend_work': {'reused_link': True}, 'commands': [], 'native_codegen': {
                    **report['native_codegen'], 'object_reports': [], 'final_artifact': artifact,
                    'object_cache': {'hits': 0, 'misses': 0, 'stored': 0}}}
                carried = runner.zil_inventory(args, retained, cache, llvm, 'c'*64)
                self.assertEqual((carried['object_count'], carried['current_object_report_count']), (16, 0))
                self.assertIsNone(carried['objects'][0]['cache_observation'])
                self.assertEqual(carried['objects'][0]['producer_cache_observation']['state'], 'miss')
                self.assertEqual(carried['compiler_summary']['object_cache']['hits'], 0)
                for key, value in [('current_preflight_checked_units', None), ('current_preflight_reused_units', 15),
                                   ('current_preflight_checked_units', True), ('profile_id', 'e'*64),
                                   ('current_llvm_verifier_executed', None)]:
                    bad = copy.deepcopy(retained)
                    bad['native_codegen']['final_artifact'][key] = value
                    with self.subTest(key=key), self.assertRaises(ValueError):
                        runner.zil_inventory(args, bad, cache, llvm, 'c'*64)
                missing = copy.deepcopy(retained)
                del missing['native_codegen']['final_artifact']['current_preflight_checked_units']
                with self.assertRaises(ValueError):
                    runner.zil_inventory(args, missing, cache, llvm, 'c'*64)
                with self.assertRaises(ValueError):
                    runner.zil_inventory(args, retained, cache, llvm, 'd'*64)
                late = {'backend_work': {'reused_link': True}, 'commands': [], 'native_codegen': {
                    **report['native_codegen'], 'final_artifact': dict(artifact, retained_files=False, before_codegen=False,
                        current_preflight_checked_units=None, current_preflight_reused_units=None, current_llvm_verifier_executed=None)}}
                current = runner.zil_inventory(args, late, cache, llvm, 'c'*64)
                self.assertEqual(current['current_object_report_count'], 16)
                self.assertEqual(current['objects'][0]['cache_observation']['state'], 'miss')
                bad_late = copy.deepcopy(late)
                bad_late['native_codegen']['final_artifact']['current_preflight_checked_units'] = 0
                with self.assertRaises(ValueError):
                    runner.zil_inventory(args, bad_late, cache, llvm, 'c'*64)
                reports[0]['cache']['action_key'], reports[1]['cache']['action_key'] = reports[1]['cache']['action_key'], reports[0]['cache']['action_key']
                with self.assertRaisesRegex(ValueError, 'ownership'):
                    runner.zil_inventory(self.args(out), report, cache, llvm, 'c'*64)

    def test_error_preserves_stale_zil_executable_without_executing_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            directory = out/'block'
            leaf = out/'request'
            (directory/'baseline').mkdir(parents=True)
            program = directory/'baseline/program'
            program.write_bytes(b'\x7fELF'+b'previous successful executable'*4)
            before = runner.publication_state([program])
            source = directory/'source'
            files = work.snapshot('scalar', 'error', 20, 20, 16)
            mapping = {str(i): str(source/name) for i, name in enumerate(files)}
            file_id = next(key for key, value in mapping.items() if value == str(source/'m0.zil'))
            offset = files['m0.zil'].encode().index(b'absent_name')
            report = {'status': 'SafeReject', 'phase': 'types', 'verification_runs': 0, 'source_files': mapping,
                      'diagnostics': [{'code': 'E-UNKNOWN-NAME', 'span': {'file': file_id, 'start': offset, 'end': offset+11}}]}
            worker = types.SimpleNamespace(request=lambda leaf: ({'wall_ms': 1}, {'report': report}), runtime={})
            with patch.object(runner, 'native_outcomes', side_effect=AssertionError('must not execute old output')):
                row = runner.zil_request(self.args(out), work, {'error': {'zil': files}}, 'error', source, {}, directory,
                                         'baseline', {'baseline': worker}, {}, leaf, {'baseline': {}})
            self.assertTrue(row['valid'])
            self.assertEqual(runner.publication_state([program]), before)
            self.assertNotIn('native', row)
            self.assertNotIn('executable', row)

    def test_error_preserves_stale_rust_executable_without_executing_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            args = self.args(out)
            args.cargo = '/fixed/cargo'
            args.timeout, args.sample_ms = 900, 5
            target = out/'target'
            program = target/'release'/runner.rc.PACKAGE
            program.parent.mkdir(parents=True)
            program.write_bytes(b'\x7fELF'+b'old successful Rust executable'*4)
            before = runner.publication_state([program])
            fixture = {'error': {'rust': runner.rust_files(work, 'error', 20, 20, 16)}}
            messages = [{'reason': 'compiler-message', 'message': {'level': 'error'}},
                        {'reason': 'build-finished', 'success': False}]
            with patch.object(runner.rc, 'run_process', return_value={'timed_out': False, 'returncode': 101}), \
                 patch.object(runner.rc, 'cargo_messages', return_value=messages), \
                 patch.object(runner.rc, 'expected_error', return_value=True), \
                 patch.object(runner, 'rust_inventory', return_value={'objects': []}), \
                 patch.object(runner, 'native_outcomes', side_effect=AssertionError('must not execute old Rust output')):
                row = runner.rust_request(args, work, fixture, 'error', out/'project', target, out/'request', {}, {})
            self.assertTrue(row['valid'])
            self.assertEqual(runner.publication_state([program]), before)
            self.assertNotIn('native', row)
            self.assertNotIn('executable', row)

    def test_successful_zil_output_is_still_present_when_next_compile_begins(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            directory = out/'block'
            (directory/'baseline').mkdir(parents=True)
            program = directory/'baseline/program'
            content = b'\x7fELF'+b'preserved successful executable'*4
            program.write_bytes(content)
            llvm = directory/'baseline/program.ll'
            llvm.write_bytes(b'; ZIL verified LLVM module\n; verified-fingerprint: '+b'a'*64+b'\ndefine i32 @main() { ret i32 0 }\n')
            source = directory/'source'
            files = work.snapshot('scalar', 'restore', 20, 20, 16)
            report = {'status': 'Success', 'phase': 'link', 'verification_runs': 1,
                      'source_files': {str(i): str(source/name) for i, name in enumerate(files)},
                      'diagnostics': [], 'query_events': [{'phase': 'verify', 'disposition': 'executed'}],
                      'artifacts': {'executable': str(program), 'llvm': str(llvm)}}

            def request(leaf):
                self.assertEqual(program.read_bytes(), content)
                return {'wall_ms': 1}, {'report': report}

            worker = types.SimpleNamespace(request=request, runtime={})
            with patch.object(runner, 'zil_inventory', return_value={'object_count': 16}), \
                 patch.object(runner, 'native_outcomes', return_value=[{'valid': True}]):
                row = runner.zil_request(self.args(out), work, {'restore': {'zil': files}}, 'restore', source, {}, directory,
                                         'baseline', {'baseline': worker}, {'baseline': llvm}, out/'request', {'baseline': {}})
            self.assertTrue(row['valid'])
            self.assertEqual(program.read_bytes(), content)

    def test_invalid_executable_is_refused_before_any_native_process(self):
        with tempfile.TemporaryDirectory() as tmp:
            program = Path(tmp)/'program'
            program.write_bytes(b'corrupt output sentinel')
            with patch.object(runner.rc, 'run_process', side_effect=AssertionError('no native process')):
                with self.assertRaises(ValueError):
                    runner.native_outcomes(self.args(Path(tmp)), work, program, 'restore', Path(tmp), {})

    def test_blob_reuse_checks_all_bytes_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            item = runner.blob(out, b'complete original artifact', '.elf')
            self.assertEqual(item, runner.blob(out, b'complete original artifact', '.elf'))
            (out/item['archive']).write_bytes(b'tampered original artifact')
            with self.assertRaises(ValueError):
                runner.blob(out, b'complete original artifact', '.elf')

    def test_recovery_is_a_separate_file_and_does_not_change_performance_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            rows = out/'rows.jsonl'
            rows.write_text('original performance rows remain intact\n')
            directory, target, source = out/'block', out/'target', out/'source'
            for arm in ('baseline', 'candidate'):
                (directory/arm).mkdir(parents=True)
                (directory/arm/'program').write_bytes(b'\x7fELF'+b'good'*20)
            program = target/'release'/runner.rc.PACKAGE
            program.parent.mkdir(parents=True)
            program.write_bytes(b'\x7fELF'+b'good'*20)

            def rust_request(*args):
                program.write_bytes(b'\x7fELF'+b'good'*20)
                return {'valid': True, 'inventory': {}}

            def zil_request(args, workloads, fixtures, current, source, source_meta, directory, arm, *rest, **kwargs):
                (directory/arm/'program').write_bytes(b'\x7fELF'+b'good'*20)
                return {'valid': True}

            with patch.object(runner, 'rust_request', rust_request), patch.object(runner, 'zil_request', zil_request):
                result = runner.publication_recovery(self.args(out), work,
                           {'restore': {'zil': work.snapshot('scalar', 'restore', 20, 20, 16)}}, source,
                           directory, out/'project', target, {}, {}, {}, {}, {})
            self.assertTrue(result['valid'])
            self.assertEqual(result['requests'], 6)
            self.assertEqual(rows.read_text(), 'original performance rows remain intact\n')
            recorded = [json.loads(line) for line in (out/'publication-correctness.jsonl').read_text().splitlines()]
            self.assertEqual(len(recorded), 6)
            self.assertTrue(all(row['measured'] is False for row in recorded))


if __name__ == '__main__':
    unittest.main()
