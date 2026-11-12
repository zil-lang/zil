#!/usr/bin/env python3
"""No compiler subprocesses: evidence/session/diagnostic regression checks."""
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

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


class PushReceiptTests(unittest.TestCase):
    def report(self, mode='check'):
        groups = {name: {field: 0 for field in fields} for name, fields in pc.FIELDS.items()}
        groups['types_push_work']['body_checks'] = 425
        groups['initialization']['product_function_queries_executed'] = 425
        groups['verification_work']['raw_shape_functions_aggregated'] = 425
        groups['lowering_work']['cost_profiled'] = False
        groups['lowering_work']['functions_checked'] = 425
        return {'status': 'Success', 'phase': 'verify' if mode == 'check' else 'link',
                'verification_runs': 1,
                'query_events': [{'phase': phase, 'disposition': 'executed'} for phase in
                                 (['types', 'analyze', 'verify'] + (['lower', 'link'] if mode == 'o2' else []))],
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

    def test_actual_cold_execution_is_required_not_just_fields(self):
        for mode in ('check', 'o2'):
            pc.inspect_report(self.report(mode), mode=mode, cold=True)
        report = self.report()
        report['types_push_work']['body_checks'] = 0
        with self.assertRaisesRegex(ValueError, 'did not check any bodies'):
            pc.inspect_report(report, mode='check', cold=True)
        report = self.report()
        report['query_events'][0]['disposition'] = 'reused'
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
        for name in ('types_push_work', 'verification_work', 'lowering_work'):
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
                          first.replace(b'verified-fingerprint: ', b'verified-fingerprint:  ')):
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


if __name__=='__main__':
    unittest.main()
