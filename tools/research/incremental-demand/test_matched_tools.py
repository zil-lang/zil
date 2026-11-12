#!/usr/bin/env python3
"""No compiler subprocesses: evidence/session/diagnostic regression checks."""
import copy
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

import matched_measure as mm
import rust_compare as rc
import push_counters as pc
import matched_summary as summary


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.out = self.root/'out'
        self.out.mkdir()
        self.args = SimpleNamespace(out=self.out, mode='o2', nm='/usr/bin/nm')
        self.target = self.root/'target'
        self.incremental = self.target/'release'/'incremental'
        self.symbol_patch = patch.object(mm, 'symbols', return_value={'path':'mock','sha256':'mock'})
        self.symbol_patch.start()
        self.addCleanup(self.symbol_patch.stop)

    def session(self, name, body=b'object'):
        directory = self.incremental/(rc.PACKAGE+'-hash')/name
        directory.mkdir(parents=True)
        (directory/'work-products.bin').write_bytes(b'work-products')
        (directory/'a.o').write_bytes(body)
        return directory

    def test_new_session_selects_current_while_old_remains(self):
        old = self.session('s-old')
        first = mm.rust_inventory(self.args,self.target,{})
        new = self.session('s-new',b'changed object')
        second = mm.rust_inventory(self.args,self.target,first)
        self.assertEqual(second['current_session'],str(new.relative_to(self.incremental)))
        self.assertEqual(second['object_count'],1)
        self.assertEqual(len(second['historical_objects']),1)
        self.assertEqual(second['historical_objects'][0]['path'],str((old/'a.o').relative_to(self.incremental)))

    def test_unchanged_session_preserves_identity(self):
        self.session('s-one')
        first = mm.rust_inventory(self.args,self.target,{})
        second = mm.rust_inventory(self.args,self.target,first)
        self.assertTrue(second['objects'][0]['same_inode_bytes_mtime_as_previous_request'])
        self.assertEqual(first['current_session'],second['current_session'])

    def test_equal_bytes_different_object_is_not_physical_reuse(self):
        self.session('s-one')
        first = mm.rust_inventory(self.args,self.target,{})
        self.session('s-two')
        second = mm.rust_inventory(self.args,self.target,first)
        self.assertTrue(second['objects'][0]['same_bytes_as_previous_request'])
        self.assertFalse(second['objects'][0]['same_inode_bytes_mtime_as_previous_request'])

    def test_ambiguous_sessions_fail_instead_of_guessing(self):
        self.session('s-one')
        self.session('s-two')
        with self.assertRaisesRegex(ValueError,'ambiguous'):
            mm.rust_inventory(self.args,self.target,{})

    def test_zil_uses_current_action_receipt_not_cache_size(self):
        cache=self.root/'cache'
        cache.mkdir()
        key='12'*32
        data=b'object payload'
        header=b'ZILOBJ01'+bytes.fromhex(key)+bytes.fromhex(rc.sha(data))+len(data).to_bytes(8,'little')
        namespace=cache/'objects-v1'/('ab'*32)
        namespace.mkdir(parents=True)
        (namespace/(key+'.zobj')).write_bytes(header+data)
        (cache/'old-unrelated.zobj').write_bytes(b'old object')
        report={'native_codegen':{'actual_units':1,'object_reports':[{'cache':{'action_key':key,'state':'hit'}}]}}
        result=mm.zil_inventory(self.args,report,cache)
        self.assertEqual(result['object_count'],1)
        self.assertEqual(result['namespace'],'ab'*32)
        self.assertEqual(result['objects'][0]['object']['sha256'],rc.sha(data))
        self.assertEqual((self.out/result['objects'][0]['object']['archive']).read_bytes(),data)

    def test_zil_rejects_ambiguous_namespace_and_corrupt_container(self):
        cache=self.root/'cache'
        namespace=cache/'objects-v1'/('ab'*32)
        namespace.mkdir(parents=True)
        key='12'*32
        data=b'object payload'
        header=b'ZILOBJ01'+bytes.fromhex(key)+bytes.fromhex(rc.sha(data))+len(data).to_bytes(8,'little')
        path=namespace/(key+'.zobj')
        path.write_bytes(header+data)
        report={'native_codegen':{'actual_units':1,'object_reports':[{'cache':{'action_key':key,'state':'hit'}}]}}
        other=cache/'objects-v1'/('cd'*32)
        other.mkdir()
        with self.assertRaisesRegex(ValueError,'exactly one namespace'):
            mm.zil_inventory(self.args,report,cache)
        other.rmdir()
        path.write_bytes(header+b'corrupt object')
        with self.assertRaisesRegex(ValueError,'header/key/payload checksum/length'):
            mm.zil_inventory(self.args,report,cache)

    def test_zil_rejects_symlinked_namespace(self):
        cache=self.root/'cache'
        schema=cache/'objects-v1'
        schema.mkdir(parents=True)
        outside=self.root/'outside'
        outside.mkdir()
        (schema/('ab'*32)).symlink_to(outside,target_is_directory=True)
        report={'native_codegen':{'actual_units':1,'object_reports':[{'cache':{'action_key':'12'*32,'state':'hit'}}]}}
        with self.assertRaisesRegex(ValueError,'unsafe Zil object cache namespace'):
            mm.zil_inventory(self.args,report,cache)

    def test_precise_zil_error_rejects_wrong_source_and_offset(self):
        source=self.root/'source'
        files={'m0.zil':'return absent_name'}
        report={'diagnostics':[{'code':'E-UNKNOWN-NAME','span':{'file':1,'start':7,'end':18}}],
                'source_files':{'1':str(source/'m0.zil')}}
        self.assertTrue(mm.zil_error(report,source,files))
        report['diagnostics'][0]['span']['start']=6
        self.assertFalse(mm.zil_error(report,source,files))
        report['diagnostics'][0]['span']['start']=7
        report['source_files']['1']=str(source/'m1.zil')
        self.assertFalse(mm.zil_error(report,source,files))

    def test_final_publication_detects_creation_and_overwrite(self):
        program=self.root/'program'
        absent=mm.publication_state([program])
        program.write_bytes(b'old executable')
        present=mm.publication_state([program])
        self.assertNotEqual(absent,present)
        self.assertEqual(present,mm.publication_state([program]))
        program.write_bytes(b'changed executable')
        self.assertNotEqual(present,mm.publication_state([program]))
        self.assertTrue(program.exists())

    def test_check_has_no_fabricated_codegen_inventory(self):
        self.args.mode='check'
        inventory=mm.rust_inventory(self.args,self.target,{})
        self.assertEqual(inventory['objects'],[])
        self.assertIsNone(inventory['current_session'])


class RuntimeEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        directory = Path(__file__).resolve().parents[1] / 'reactive-query-graph'
        workloads = mm.load_module('runtime_test_workloads', directory / 'native_workloads.py')
        with patch.dict('sys.modules', {'native_workloads': workloads}):
            cls.native = mm.load_module('runtime_test_native', directory / 'native_measure.py')

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.out = self.root / 'out'
        self.out.mkdir()
        self.sdk = self.root / 'SDK with spaces'
        self.sdk.mkdir()
        self.library = self.sdk / 'libLLVM.so.23.1'
        self.library.write_bytes(b'original LLVM bytes')
        self.env = self.native.llvm_environment(self.sdk, {'LD_LIBRARY_PATH': '/wrong/arm'})

    def maps(self, path=None, inode=None):
        path = path or self.library
        stat = path.stat()
        device = f'{os.major(stat.st_dev):02x}:{os.minor(stat.st_dev):02x}'
        escaped = str(path).replace(' ', r'\040')
        return (f'1000-2000 r--p 00000000 {device} {stat.st_ino if inode is None else inode} {escaped}\n'
                f'2000-3000 r-xp 00001000 {device} {stat.st_ino if inode is None else inode} {escaped}\n').encode()

    def capture(self, data=None, cache=None):
        original = Path.read_bytes
        with patch.object(Path, 'read_bytes', lambda path: self.maps() if str(path) == '/proc/123/maps' and data is None
                          else data if str(path) == '/proc/123/maps' else original(path)):
            return self.native.capture_runtime(123, self.out, self.env, {} if cache is None else cache)

    def test_explicit_arm_path_replaces_inherited_loader_path_and_rejects_injection(self):
        self.assertEqual(self.env['LD_LIBRARY_PATH'], str(self.sdk))
        self.assertEqual(self.env['ZIL_LLVM_LIB_DIR'], str(self.sdk))
        for key in ('LD_PRELOAD', 'LD_AUDIT'):
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'not permitted'):
                self.native.llvm_environment(self.sdk, {key: '/other/injection.so'})
        with self.assertRaises(FileNotFoundError):
            self.native.llvm_environment(self.root / 'missing', {})

    def test_actual_maps_deduplicates_segments_and_archives_exact_dso(self):
        receipt = self.capture()
        self.assertEqual(len(receipt['llvm_dsos']), 1)
        item = receipt['llvm_dsos'][0]
        self.assertEqual(item['path'], str(self.library))
        self.assertEqual((self.out / item['archive']).read_bytes(), self.library.read_bytes())
        self.assertEqual(mm.file_hash(self.out / item['archive']), item['sha256'])
        self.assertEqual((self.out / receipt['maps']['archive']).read_bytes(), self.maps())

    def test_no_mapped_llvm_wrong_arm_deleted_or_stale_inode_are_rejected(self):
        other = self.root / 'libLLVM.so'
        other.write_bytes(b'other arm')
        for data in (b'', self.maps(other), self.maps(inode=self.library.stat().st_ino + 1),
                     self.maps().replace(b'libLLVM.so.23.1', b'libLLVM.so.23.1 (deleted)')):
            with self.subTest(data=data), self.assertRaises(ValueError):
                self.capture(data)

    def test_stamp_cache_cannot_hide_replaced_dso_and_final_recapture_is_exact(self):
        cache = {}
        first = self.capture(cache=cache)
        unchanged = self.capture(cache=cache)
        self.assertEqual(first['llvm_dsos'], unchanged['llvm_dsos'])
        self.library.write_bytes(b'patched LLVM bytes')
        changed = self.capture(cache=cache)
        self.assertNotEqual(first['llvm_dsos'][0]['sha256'], changed['llvm_dsos'][0]['sha256'])
        final = self.capture(cache={})
        self.assertEqual(changed['llvm_dsos'], final['llvm_dsos'])


    def test_run_pin_rejects_changed_library_in_later_block_but_allows_distinct_arms(self):
        receipt = self.capture()
        manifest = {}
        self.native.bind_runtime_libraries(manifest, 'baseline', receipt)
        self.native.bind_runtime_libraries(manifest, 'baseline', self.capture())
        self.library.write_bytes(b'new candidate LLVM')
        candidate = self.capture()
        self.native.bind_runtime_libraries(manifest, 'candidate', candidate)
        with self.assertRaisesRegex(ValueError, 'between blocks'):
            self.native.bind_runtime_libraries(manifest, 'baseline', candidate)

    def test_final_capture_rehashes_without_cache_and_rejects_changed_dso(self):
        worker = self.native.Worker.__new__(self.native.Worker)
        worker.p = SimpleNamespace(pid=123)
        worker.directory, worker.runtime_root, worker.env = self.out, self.out, self.env
        worker.runtime = self.capture()
        final = copy.deepcopy(worker.runtime)
        with patch.object(self.native, 'capture_runtime', return_value=final) as capture:
            self.assertEqual(worker.finalize_runtime(), final)
            self.assertEqual(capture.call_args.args, (123, self.out, self.env, {}))
        final['llvm_dsos'][0]['sha256'] = '0' * 64
        with patch.object(self.native, 'capture_runtime', return_value=final), \
                self.assertRaisesRegex(ValueError, 'during block'):
            worker.finalize_runtime()

    def check_blob(self, item):
        path = (self.out / item['archive']).resolve()
        if (not path.is_relative_to(self.out) or mm.file_hash(path) != item['sha256']
                or path.stat().st_size != item['bytes']):
            raise ValueError('blob hash/size/path differs')

    def inspect(self, receipt, env=None, pid=123):
        return pc.inspect_runtime_receipt(receipt, self.native.runtime_environment(self.env) if env is None else env,
                                          pid, self.out, self.check_blob, self.native.llvm_mappings)

    def test_posthoc_runtime_binds_maps_bytes_pid_arm_and_kernel_file_identity(self):
        receipt = self.capture()
        self.assertEqual(self.inspect(receipt), receipt['llvm_dsos'])
        for label in ('pid', 'environment', 'missing', 'inode', 'size', 'outside', 'hash', 'map_hash'):
            altered = copy.deepcopy(receipt)
            if label == 'pid':
                altered['pid'] += 1
            elif label == 'environment':
                altered['environment']['LD_LIBRARY_PATH'] = '/other/arm'
            elif label == 'missing':
                altered['llvm_dsos'] = []
            elif label == 'inode':
                altered['llvm_dsos'][0]['file_stamp'][1] += 1
            elif label == 'size':
                altered['llvm_dsos'][0]['file_stamp'][2] += 1
            elif label == 'outside':
                altered['llvm_dsos'][0]['resolved_path'] = '/other/libLLVM.so'
            elif label == 'hash':
                altered['llvm_dsos'][0]['sha256'] = '0' * 64
            else:
                altered['maps']['sha256'] = '0' * 64
            with self.subTest(label=label), self.assertRaises(ValueError):
                self.inspect(altered)
        (self.out / receipt['llvm_dsos'][0]['archive']).write_bytes(b'corrupt archive')
        with self.assertRaisesRegex(ValueError, 'blob'):
            self.inspect(receipt)

    def test_posthoc_environment_is_explicit_and_portable_without_original_sdk(self):
        envs = {arm: {**self.native.runtime_environment(self.env),
                      'LD_LIBRARY_PATH': '/original unavailable/' + arm,
                      'ZIL_LLVM_LIB_DIR': '/original unavailable/' + arm}
                for arm in ('baseline', 'candidate')}
        manifest = {'runtime_binding_schema': 'per-arm-resident-llvm-v1',
                    'arm_runtime_environments': envs,
                    'arguments': {arm + '_llvm_library_path': value['LD_LIBRARY_PATH']
                                  for arm, value in envs.items()}}
        self.assertEqual(pc.inspect_runtime_environments(manifest), envs)
        for label in ('missing_schema', 'override', 'preload', 'relative', 'multi_directory'):
            altered = copy.deepcopy(manifest)
            if label == 'missing_schema':
                del altered['runtime_binding_schema']
            elif label == 'override':
                altered['arguments']['baseline_llvm_library_path'] = '/other/SDK'
            elif label == 'preload':
                altered['arm_runtime_environments']['candidate']['LD_PRELOAD'] = '/other/injection.so'
            else:
                value = 'relative' if label == 'relative' else '/one:/two'
                altered['arm_runtime_environments']['baseline']['LD_LIBRARY_PATH'] = value
                altered['arm_runtime_environments']['baseline']['ZIL_LLVM_LIB_DIR'] = value
                altered['arguments']['baseline_llvm_library_path'] = value
            with self.subTest(label=label), self.assertRaises(ValueError):
                pc.inspect_runtime_environments(altered)

    def test_both_workers_pass_the_exact_per_arm_environment_to_direct_popen(self):
        # Transport is mocked: this test executes neither compiler nor shell.
        process = MagicMock()
        process.pid = 123
        process.poll.return_value = None
        def ready(worker):
            worker.last_line = b'{"ready":true,"protocol":1}'
            return {'ready': True, 'protocol': 1}
        command = ['/immutable/candidate-zil', 'check', '/source/main.zil', '--resident']
        matched = self.out / 'matched'
        matched.mkdir()
        with patch.object(mm.subprocess, 'Popen', return_value=process) as popen, \
                patch.object(mm.Worker, 'read', ready), \
                patch.object(self.native, 'capture_runtime', return_value=self.capture()):
            worker = mm.Worker(command, matched, 1, 5, self.native, self.env, self.out)
            worker.close()
            self.assertEqual(popen.call_args.args, (command,))
            self.assertIs(popen.call_args.kwargs['env'], self.env)
        native = self.out / 'native'
        native.mkdir()
        with patch.object(self.native.subprocess, 'Popen', return_value=process) as popen, \
                patch.object(self.native.Worker, 'read_json', ready), \
                patch.object(self.native, 'capture_runtime', return_value=self.capture()):
            worker = self.native.Worker(command, native, 1, 5, self.env, self.out)
            worker.close()
            self.assertEqual(popen.call_args.args, (command,))
            self.assertIs(popen.call_args.kwargs['env'], self.env)


class PushReceiptTests(unittest.TestCase):
    def report(self, mode='check'):
        groups = {name: {field: 0 for field in fields} for name, fields in pc.FIELDS.items()}
        groups['raw_inference_work']['full_runs'] = 1
        groups['raw_inference_work']['closure_bodies_checked'] = 425
        groups['raw_inference_work']['name_functions_checked'] = 425
        groups['raw_inference_work']['dependency_bodies_checked'] = 425
        groups['types_push_work']['body_checks'] = 425
        groups['initialization']['product_function_queries_executed'] = 425
        groups['verification_work']['raw_shape_functions_aggregated'] = 425
        groups['lowering_work']['cost_profiled'] = False
        groups['lowering_work']['functions_checked'] = 425
        return {'status': 'Success', 'phase': 'verify' if mode == 'check' else 'link',
                'verification_runs': 1,
                'query_events': [{'phase': phase, 'disposition': 'executed'} for phase in
                                 (['ct', 'types', 'analyze', 'verify'] + (['lower', 'link'] if mode == 'o2' else []))],
                'raw_inference_work': groups['raw_inference_work'],
                'types_push_work': groups['types_push_work'],
                'frontend_stats': {'initialization': groups['initialization']},
                'verification_work': groups['verification_work'],
                'lowering_work': groups['lowering_work'] if mode == 'o2' else None}

    def test_explicit_baseline_pins_cannot_be_substituted(self):
        pc.baseline_pin(pc.MAIN, pc.MAIN)
        pc.baseline_pin(pc.LEGACY_MAIN, pc.LEGACY_MAIN)
        with self.assertRaises(ValueError):
            pc.baseline_pin(pc.LEGACY_MAIN, pc.MAIN)
        with self.assertRaises(ValueError):
            pc.baseline_pin('a' * 40, 'a' * 40)
        self.assertEqual(mm.MAIN, pc.MAIN)

    def test_only_declared_candidate_flags_enter_formal_command(self):
        context = {'binary': '/compiler/zil', 'mode': 'check',
                   'source': '/original run/block-00/source/main.zil',
                   'work': '/original run/block-00/candidate'}
        command = ['/compiler/zil', 'check', '/original run/block-00/source/main.zil',
                   '--json', '--resident', '--resident-capacity', '256',
                   '--resident-reset-after', '4096', '--semantic-cache',
                   '/original run/block-00/candidate/semantic', '--push-reuse']
        self.assertEqual(pc.zil_command(arm='candidate', extras=['--push-reuse'], **context), command)
        pc.command_flags(command, 'candidate', ['--push-reuse'], **context)
        for extras in [['--profile-compilation'], ['--codegen-units=1'], ['--push-reuse'] * 2, ['--opt=O0']]:
            with self.subTest(extras=extras), self.assertRaises(ValueError):
                pc.candidate_flags(extras)
        for extra in [['--max-nodes', '0'], ['--max-analysis-steps=1'], ['--closed-executable'],
                      ['--opt=O0'], ['--codegen-units=1'], ['--profile-compilation'],
                      ['--incremental-o2'], ['--push-reuse'], ['--resident'], ['unexpected.zil']]:
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                pc.command_flags(command + extra, 'candidate', ['--push-reuse'], **context)
        for index, value in [(0, '/other/zil'), (2, '/other/main.zil'),
                             (6, '128'), (10, '/other/cache')]:
            changed = command.copy()
            changed[index] = value
            with self.subTest(index=index), self.assertRaises(ValueError):
                pc.command_flags(changed, 'candidate', ['--push-reuse'], **context)
        for changed, arm in [(command, 'baseline'), (command[:-1], 'candidate'),
                             ([], 'candidate'), (None, 'candidate')]:
            with self.subTest(changed=changed, arm=arm), self.assertRaises(ValueError):
                pc.command_flags(changed, arm, ['--push-reuse'], **context)

    def test_native_argv_binds_every_flag_and_both_artifact_paths(self):
        context = {'binary': '/compiler/zil', 'mode': 'o2', 'source': '/run/source/main.zil',
                   'work': '/run/candidate'}
        command = pc.zil_command(arm='candidate', extras=['--push-reuse'], **context)
        self.assertEqual(command[11:], ['--backend', 'llvm-api', '--opt', 'O2',
                                       '--codegen-units', '16', '--codegen-jobs', '1',
                                       '--object-cache', '/run/candidate/objects',
                                       '-o', '/run/candidate/program', '--push-reuse'])
        for flag, replacement in [('--backend', 'clang'), ('--opt', 'O0'),
                                  ('--codegen-units', '1'), ('--codegen-jobs', '4'),
                                  ('--object-cache', '/other/objects'), ('-o', '/other/program')]:
            changed = command.copy()
            changed[changed.index(flag) + 1] = replacement
            with self.subTest(flag=flag), self.assertRaises(ValueError):
                pc.command_flags(changed, 'candidate', ['--push-reuse'], **context)
        with self.assertRaises(ValueError):
            pc.command_flags(command + ['--opt', 'O0'], 'candidate', ['--push-reuse'], **context)

    def test_posthoc_argv_is_bound_after_archive_relocation_without_original_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            restored = Path(temporary) / 'restored archive'
            original = Path('/original unavailable archive')
            manifest = {'arguments': {'out': str(original), 'mode': 'o2',
                                      'candidate_extra_arg': ['--push-reuse']},
                        'binaries': {arm: {'path': '/compiler/' + arm} for arm in ('baseline', 'candidate')}}
            for block in range(6):
                for arm in ('baseline', 'candidate'):
                    directory = restored / f'block-{block:02d}' / arm
                    directory.mkdir(parents=True)
                    command = pc.zil_command(binary=manifest['binaries'][arm]['path'], mode='o2',
                                             source=original / f'block-{block:02d}' / 'source/main.zil',
                                             work=original / f'block-{block:02d}' / arm, arm=arm,
                                             extras=['--push-reuse'])
                    (directory / 'command.json').write_text(json.dumps({'argv': command}))
            pc.inspect_commands(restored, manifest)
            path = restored / 'block-04/baseline/command.json'
            record = json.loads(path.read_text())
            record['argv'] += ['--max-values', '1']
            path.write_text(json.dumps(record))
            with self.assertRaises(ValueError):
                pc.inspect_commands(restored, manifest)

    def test_raw_inference_is_required_only_for_successful_current_scalar_ct(self):
        report = self.report()
        self.assertTrue(pc.inspect_report(report, mode='check')['required_by_execution']['raw_inference_work'])
        report['raw_inference_work'] = None
        with self.assertRaisesRegex(ValueError, 'raw_inference_work unavailable'):
            pc.inspect_report(report, mode='check')
        report['query_events'][0]['disposition'] = 'reused'
        checked = pc.inspect_report(report, mode='check')
        self.assertIsNone(checked['groups']['raw_inference_work'])
        self.assertFalse(checked['required_by_execution']['raw_inference_work'])
        error = {'status': 'SafeReject', 'phase': 'types', 'verification_runs': 0,
                 'query_events': [{'phase': 'ct', 'disposition': 'executed'},
                                  {'phase': 'types', 'disposition': 'executed'}]}
        checked = pc.inspect_report(error, mode='check')
        self.assertIsNone(checked['groups']['raw_inference_work'])
        self.assertFalse(checked['required_by_execution']['raw_inference_work'])

    def test_projection_schema_preserves_legacy_unknown_and_requires_current_observations(self):
        legacy = self.native_stats()
        report = {'native_codegen': {'object_reports': [{'native_products': legacy}]}}
        observed = pc.inspect_native_products(report)['units'][0]['stats']
        self.assertIsNone(observed.get('world_projection_direct'))
        current = dict(legacy, observer_schema=2)
        current.update({field: 7 for field in pc.NATIVE_PROJECTION_FIELDS})
        report['native_codegen']['object_reports'][0]['native_products'] = current
        self.assertEqual(pc.inspect_native_products(report)['units'][0]['stats'], current)
        for field in pc.NATIVE_PROJECTION_FIELDS:
            for value in (None, -1, True):
                malformed = dict(current)
                malformed[field] = value
                report['native_codegen']['object_reports'][0]['native_products'] = malformed
                with self.subTest(field=field, value=value), self.assertRaisesRegex(ValueError, field):
                    pc.inspect_native_products(report)
        for version in (True, 0, 3):
            report['native_codegen']['object_reports'][0]['native_products'] = dict(current, observer_schema=version)
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, 'observer_schema'):
                pc.inspect_native_products(report)

    def native_stats(self):
        return {field: 0 for field in pc.NATIVE_PRODUCT_FIELDS}

    def test_native_mir_scopes_and_fallback_counts_are_actual_required_integer_fields(self):
        stats = self.native_stats()
        stats.update({'available': 1, 'mir_key_ns': 11, 'mir_parse_ns': 12,
                      'mir_restore_validate_ns': 13, 'mir_capture_ns': 14,
                      'fresh_context_fallbacks': 1, 'fallback_ns': 1000,
                      'machine_function_reuses': 20})
        report = self.report('o2')
        report['native_codegen'] = {'object_reports': [{'native_products': stats},
                                                      json.dumps({'native_products': stats}), {}]}
        result = pc.inspect_report(report, mode='o2')['native_products']
        self.assertEqual(result['units'][0]['stats'], stats)
        self.assertEqual(result['units'][1]['stats']['mir_parse_ns'], 12)
        self.assertIsNone(result['units'][2]['stats'])
        self.assertIn('first hooked attempt only', result['scope'])
        self.assertIn('all driver retries', result['scope'])
        for field in pc.NATIVE_PRODUCT_FIELDS:
            for value in (None, -1, True, 1.5, '0'):
                malformed = dict(stats)
                malformed[field] = value
                report['native_codegen']['object_reports'] = [{'native_products': malformed}]
                with self.subTest(field=field, value=value), self.assertRaisesRegex(ValueError, 'native_products.' + field):
                    pc.inspect_report(report, mode='o2')
            malformed = dict(stats)
            del malformed[field]
            report['native_codegen']['object_reports'] = [{'native_products': malformed}]
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'native_products.' + field):
                pc.inspect_report(report, mode='o2')

    def test_summary_preserves_raw_ct_and_native_unknowns_without_counting_discarded_reuse_as_avoided(self):
        report = self.report('o2')
        stats = self.native_stats()
        stats.update({'fresh_context_fallbacks': 1, 'machine_function_reuses': 7, 'mir_capture_ns': 99})
        report['native_codegen'] = {'object_reports': [{'native_products': stats}, {}]}
        result = summary.reuse({'resident': {'report': report}})
        self.assertEqual(result['raw_inference_work']['closure_bodies_checked'], 425)
        self.assertEqual(result['native_products']['units'][0]['stats']['machine_function_reuses'], 7)
        self.assertIsNone(result['native_products']['units'][1]['stats'])
        self.assertIn('discarded attempt', result['native_products']['scope'])
        report['raw_inference_work'] = None
        report['native_codegen'] = None
        result = summary.reuse({'resident': {'report': report}})
        self.assertIsNone(result['raw_inference_work'])
        self.assertIsNone(result['native_products'])

    def test_actual_cold_execution_is_required_not_just_fields(self):
        for mode in ('check', 'o2'):
            pc.inspect_report(self.report(mode), mode=mode, cold=True)
        report = self.report()
        report['types_push_work']['body_checks'] = 0
        with self.assertRaisesRegex(ValueError, 'did not check any bodies'):
            pc.inspect_report(report, mode='check', cold=True)
        report = self.report()
        report['query_events'][1]['disposition'] = 'reused'
        with self.assertRaisesRegex(ValueError, 'did not execute'):
            pc.inspect_report(report, mode='check', cold=True)

    def test_whole_frontend_hit_does_not_fabricate_zero_work(self):
        report = self.report()
        report['types_push_work'] = None
        report['frontend_stats']['initialization'] = None
        report['query_events'] = [{'phase': 'verify', 'disposition': 'executed'}]
        checked = pc.inspect_report(report, mode='check')
        self.assertIsNone(checked['groups']['types_push_work'])
        self.assertIsNone(checked['groups']['initialization'])
        self.assertFalse(checked['required_by_execution']['types_push_work'])

    def test_null_and_absent_are_preserved_and_execution_requires_receipt(self):
        for name in ('raw_inference_work', 'types_push_work', 'verification_work', 'lowering_work'):
            for absent in (False, True):
                report = self.report('o2')
                if absent:
                    del report[name]
                else:
                    report[name] = None
                with self.subTest(name=name, absent=absent), self.assertRaisesRegex(ValueError, 'unavailable'):
                    pc.inspect_report(report, mode='o2')
        report = self.report()
        report['frontend_stats']['initialization'] = None
        with self.assertRaisesRegex(ValueError, 'initialization unavailable'):
            pc.inspect_report(report, mode='check')

    def test_partial_receipts_invalid_numbers_and_omitted_foreign_are_errors(self):
        for name, fields in pc.FIELDS.items():
            for field in fields:
                for invalid in (None, -1, True, 1.5, '0'):
                    report = self.report('o2')
                    group = report['frontend_stats'][name] if name == 'initialization' else report[name]
                    group[field] = invalid
                    with self.subTest(name=name, field=field, invalid=invalid), self.assertRaises(ValueError):
                        pc.inspect_report(report, mode='o2')
        report = self.report()
        del report['verification_work']['foreign_functions_selected']
        with self.assertRaisesRegex(ValueError, 'foreign_functions_selected missing'):
            pc.inspect_report(report, mode='check')

    def test_early_type_failure_allows_unavailable_work_but_no_current_verify(self):
        report = {'status': 'SafeReject', 'phase': 'types', 'verification_runs': 0,
                  'query_events': [{'phase': 'types', 'disposition': 'executed'}]}
        checked = pc.inspect_report(report, mode='check')
        self.assertTrue(all(value is None for value in checked['groups'].values()))
        self.assertTrue(all(value is False for value in checked['fields_present'].values()))
        report['verification_runs'] = 1
        with self.assertRaisesRegex(ValueError, 'verification_work unavailable'):
            pc.inspect_report(report, mode='check')

    def test_llvm_comparison_excludes_only_the_strict_identity_value(self):
        body = b'target triple = "x86_64-unknown-linux-gnu"\ndefine i32 @main() { ret i32 0 }\n'
        def module(identity, tail=body):
            return b'; ZIL verified LLVM module\n; verified-fingerprint: ' + identity + b'\n' + tail
        first, second = module(b'a' * 64), module(b'b' * 64)
        self.assertNotEqual(rc.sha(first), rc.sha(second))
        self.assertEqual(pc.llvm_comparison_sha256(first), pc.llvm_comparison_sha256(second))
        for tail in (body.replace(b'i32 0', b'i32 1'), body.replace(b'x86_64', b'aarch64'), body + b'; extra comment\n'):
            self.assertNotEqual(pc.llvm_comparison_sha256(first), pc.llvm_comparison_sha256(module(b'b' * 64, tail)))
        for malformed in (module(b'g' * 64), module(b'a' * 63), first.replace(b'\n', b'\r\n'), b'\n' + first,
                          first.replace(b'verified-fingerprint: ', b'verified-fingerprint:  '),
                          first + b'; verified-fingerprint: ' + b'c' * 64 + b'\n',
                          b'; header\n' + first, first.replace(b'; ZIL verified LLVM module\n', b'; ZIL verified LLVM module\n\n')):
            with self.subTest(malformed=malformed[:100]), self.assertRaises(ValueError):
                pc.llvm_comparison_sha256(malformed)

    def test_profiled_lower_counters_never_validate_as_formal_wall(self):
        report = self.report('o2')
        report['lowering_work']['cost_profiled'] = True
        with self.assertRaisesRegex(ValueError, 'timing mode'):
            pc.inspect_report(report, mode='o2')
        pc.inspect_report(report, mode='o2', profiled=True)

    def test_summary_keeps_real_consumers_and_unknown_fields_distinct_from_zero(self):
        report = self.report('o2')
        result = summary.reuse({'resident': {'report': report}})
        self.assertEqual(result['types_push_work']['body_checks'], 425)
        self.assertEqual(result['initialization']['product_function_queries_executed'], 425)
        self.assertEqual(result['lowering_work']['functions_checked'], 425)
        del report['frontend_stats']['initialization']['product_function_queries_executed']
        report['types_push_work'] = None
        result = summary.reuse({'resident': {'report': report}})
        self.assertIsNone(result['types_push_work'])
        self.assertIsNone(result['initialization']['product_function_queries_executed'])


class CargoCommandTests(unittest.TestCase):
    def resolution(self, root, *, direct=False):
        actual, launcher = '/pinned/toolchain/bin/cargo', '/pinned/proxy/rustup'
        if direct:
            launcher = actual
        manifest = {'arguments': {'cargo': 'cargo'},
                    'tools': {'cargo': {'requested': 'cargo', 'launcher_path': launcher,
                                        'launcher_sha256': 'a' * 64, 'actual_path': actual,
                                        'actual_sha256': 'a' * 64 if direct else 'b' * 64}},
                    'tools_after_sha256': {'cargo': 'a' * 64 if direct else 'b' * 64}}
        def evidence(name, argv, stdout):
            directory = root / name
            directory.mkdir()
            (directory / 'command.json').write_text(json.dumps({'argv': argv}))
            (directory / 'stdout.bin').write_bytes(stdout)
            (directory / 'stderr.bin').write_bytes(b'')
            (directory / 'result.json').write_text(json.dumps({'returncode': 0, 'timed_out': False,
                'stdout_sha256': rc.sha(stdout), 'stderr_sha256': rc.sha(b'')}))
        if not direct:
            evidence('cargo-resolution', [launcher, 'which', 'cargo'], (actual + '\n').encode())
        evidence('cargo-version', [actual, '-vV'], b'cargo 1.99.1 pinned version\n')
        return manifest

    def test_logical_cargo_resolves_to_exact_pinned_argv_and_never_an_argv0_wildcard(self):
        for direct in (False, True):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                manifest = self.resolution(root, direct=direct)
                cargo = pc.resolved_tool(root, manifest, 'cargo')
                expected = pc.rust_command(cargo=cargo, mode='o2', target='/run/rust-target')
                pc.rust_command_flags(expected, cargo=cargo, mode='o2', target='/run/rust-target')
                for replacement in ('cargo', '/different/cargo', '/pinned/proxy/rustup', '', '/bin/sh'):
                    command = expected.copy()
                    command[0] = replacement
                    with self.subTest(direct=direct, replacement=replacement), self.assertRaises(ValueError):
                        pc.rust_command_flags(command, cargo=cargo, mode='o2', target='/run/rust-target')
                for extra in (['--jobs', '16'], ['--release'], ['--profile', 'dev']):
                    with self.subTest(extra=extra), self.assertRaises(ValueError):
                        pc.rust_command_flags(expected + extra, cargo=cargo, mode='o2', target='/run/rust-target')

    def test_missing_or_inconsistent_tool_resolution_and_raw_evidence_are_rejected(self):
        for mutation in ('missing_tool', 'missing_actual', 'bad_path', 'bad_hash', 'requested', 'after',
                         'resolution_argv', 'resolution_output', 'resolution_failed',
                         'version_argv', 'version_output', 'missing_resolution'):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                manifest = self.resolution(root)
                if mutation == 'missing_tool': del manifest['tools']['cargo']
                elif mutation == 'missing_actual': del manifest['tools']['cargo']['actual_path']
                elif mutation == 'bad_path': manifest['tools']['cargo']['actual_path'] = 'relative/cargo'
                elif mutation == 'bad_hash': manifest['tools']['cargo']['actual_sha256'] = 'bad'
                elif mutation == 'requested': manifest['tools']['cargo']['requested'] = '/other/cargo'
                elif mutation == 'after': manifest['tools_after_sha256']['cargo'] = '0' * 64
                elif mutation.endswith('_argv'):
                    directory = 'cargo-version' if mutation.startswith('version') else 'cargo-resolution'
                    path = root / directory / 'command.json'
                    data = json.loads(path.read_text()); data['argv'][0] = '/other/cargo'
                    path.write_text(json.dumps(data))
                elif mutation.endswith('_output'):
                    directory = 'cargo-version' if mutation.startswith('version') else 'cargo-resolution'
                    (root / directory / 'stdout.bin').write_bytes(b'/other/cargo\n')
                elif mutation == 'resolution_failed':
                    path = root / 'cargo-resolution/result.json'
                    data = json.loads(path.read_text()); data['returncode'] = 1
                    path.write_text(json.dumps(data))
                else:
                    (root / 'cargo-resolution/command.json').unlink()
                with self.subTest(mutation=mutation), self.assertRaises((ValueError, OSError)):
                    pc.resolved_tool(root, manifest, 'cargo')

    def test_modes_and_target_are_complete_and_fixed(self):
        common = ['/tool/cargo', 'check', '--offline', '--locked', '--jobs', '1',
                  '--verbose', '--message-format=json', '--target-dir', '/run/rust-target']
        self.assertEqual(pc.rust_command(cargo='/tool/cargo', mode='check', target='/run/rust-target'), common)
        common[1] = 'build'
        self.assertEqual(pc.rust_command(cargo='/tool/cargo', mode='o2', target='/run/rust-target'), common + ['--release'])
        with self.assertRaises(ValueError):
            pc.rust_command(cargo='/tool/cargo', mode='o2', target='/run/../other')


if __name__=='__main__':
    unittest.main()
