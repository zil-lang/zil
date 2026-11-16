#!/usr/bin/env python3
"""Small functional harness for root to run; no compiler/benchmark needed."""
import copy
import json
from pathlib import Path
import struct
import sys
import tempfile
import threading
import types
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent))
import closed_project_matrix as matrix
import closed_project_quality as quality


class ClosedProjectTests(unittest.TestCase):
    def args(self, out=None, preset='1000'):
        return types.SimpleNamespace(out=out, repo=matrix.ROOT, preset=preset, modules=100,
            shape=matrix.SHAPES[preset], graph='wide', mode='o2', native_repetitions=1,
            candidate_policy='closed', function_merging='off',
            baseline=Path('/fixed/main'), candidate=Path('/fixed/current'),
            native_link_profile=Path('/fixed/profile'), native_link_profile_sha256='f'*64)

    def test_original_fixtures_and_interface_and_runtime_wrapper(self):
        workloads = matrix.rc.load_workloads(matrix.ROOT)
        requests = matrix.history(8)
        self.assertEqual(len(requests), 18)
        self.assertEqual(requests[-5]['name'], 'broad')
        self.assertEqual([r['name'] for r in requests[-4:]], ['error', 'recovery', 'restore', 'warm-restart'])
        for preset in matrix.SIZES:
            args = self.args(preset=preset)
            cold, meta = matrix.fixture(args, workloads, requests[0])
            self.assertEqual(meta['shape'], 'simple' if preset == '2500' else 'short')
            for language, expected in matrix.COLD_IDENTITIES[preset].items():
                identity = meta['source'][language]
                self.assertEqual((identity['source_files'], identity['source_bytes'], identity['identity_sha256']), expected)
            if preset == '5105':
                self.assertEqual((meta['source']['zil']['source_files'], meta['source']['zil']['source_bytes'],
                                  meta['source']['zil']['identity_sha256']), (102, 843875, matrix.ORIGINAL5105))
                self.assertNotIn('unused_checked', cold['zil']['m0.zil'])
            else:
                old, _ = matrix.large.fixture(args, workloads, requests[0])
                self.assertEqual(old, cold)
            interface = next(r for r in requests if r['name'] == 'interface')
            edited, _ = matrix.fixture(args, workloads, interface)
            self.assertIn('value(x: i32, offset: i32)', edited['zil']['m0.zil'])
            self.assertIn('m0.value(x, 0)', edited['zil']['main.zil'])
            wrapped, wrapped_meta = matrix.fixture(args, workloads, interface, True)
            self.assertTrue(wrapped_meta['wrapper_only_main_changed'])
            self.assertIn('while iteration < 10000 {', wrapped['zil']['main.zil'])
            self.assertIn('while iteration < 10000 {', wrapped['rust']['src/main.rs'])
            for lang in ('zil', 'rust'):
                self.assertEqual({n: t for n, t in edited[lang].items() if n not in ('main.zil', 'src/main.rs')},
                                 {n: t for n, t in wrapped[lang].items() if n not in ('main.zil', 'src/main.rs')})

    def test_explicit_candidate_flags_never_reach_main(self):
        args = self.args()
        main = matrix.zil_command(args, 'baseline', Path('/src/main.zil'), Path('/work/baseline'))
        candidate = matrix.zil_command(args, 'candidate', Path('/src/main.zil'), Path('/work/candidate'))
        self.assertNotIn('--push-reuse', main)
        self.assertNotIn('--closed-executable', main)
        self.assertNotIn('--function-merging', main)
        self.assertIn('--closed-executable', candidate)
        self.assertEqual(candidate[-2:], ['--function-merging', 'off'])
        args.candidate_policy = 'late'
        with self.assertRaises(ValueError):
            quality.policy(args)
        args.candidate_policy = None
        with self.assertRaises(ValueError):
            matrix.zil_command(args, 'candidate', Path('/src/main.zil'), Path('/work/candidate'))

    def test_unsupported_formal_policy_refused_before_output_or_process(self):
        with tempfile.TemporaryDirectory() as tmp:
            for closed, merging in (('late', 'off'), ('closed', 'after'), ('closed', 'before-and-after')):
                output = Path(tmp)/'never-created'
                with patch.object(matrix.matched, 'Worker') as worker, self.assertRaises(ValueError):
                    matrix.main(['measure', '--out', str(output), '--pins', '/missing/pins.json',
                                 '--candidate-policy', closed, '--function-merging', merging])
                worker.assert_not_called()
                self.assertFalse(output.exists())

    def test_hit_requires_current_machine_preflight_and_exact_scope(self):
        good = {'input_kind': 'object-cache', 'llvm_verification_this_request': False,
                'producer_verified_before_after': True, 'current_machine_preflight_reused': True,
                'preflight_ns': 19, 'cache': {'state': 'hit', 'current_preflight_ns': 19},
                'builder_ns': 0, 'parse_ns': 0, 'optimize_ns': 0, 'emit_ns': 0}
        self.assertTrue(quality.hit_preflight(good))
        for field, bad in (('input_kind', 'builder'), ('llvm_verification_this_request', True),
                           ('producer_verified_before_after', False), ('current_machine_preflight_reused', 1),
                           ('preflight_ns', 18), ('builder_ns', 1)):
            value = copy.deepcopy(good)
            value[field] = bad
            with self.subTest(field=field), self.assertRaises(ValueError):
                quality.hit_preflight(value)
        for field in ('input_kind', 'current_machine_preflight_reused', 'producer_verified_before_after'):
            value = copy.deepcopy(good)
            del value[field]
            with self.assertRaises(ValueError):
                quality.hit_preflight(value)

    def test_producer_archive_is_rehashed_on_hit_and_retained_consumption(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            args = self.args(out)
            receipt = {'backend': 'llvm-api', 'pipeline': 'default<O2>', 'triple': 'x86_64-unknown-linux-gnu',
                       'llvm_version': '23.1.4', 'verify_before_after': True, 'input_kind': 'builder',
                       'object_bytes': 9, 'cache': {'state': 'miss', 'action_key': 'a'*64, 'corrupt': False}}
            archive = matrix.matched.blob(out, json.dumps({'receipt': receipt, 'object_sha256': 'b'*64, 'policy': 'closed+off'}).encode(), '.producer.json')
            item = {'action_key': 'a'*64, 'object': {'sha256': 'b'*64, 'bytes': 9}, 'producer_native_receipt': archive}
            quality.producer_receipt(args, item)
            mismatch = copy.deepcopy(item)
            mismatch['object']['sha256'] = 'c'*64
            with self.assertRaises(ValueError):
                quality.producer_receipt(args, mismatch)
            (out/archive['archive']).write_bytes(b'corrupt archived producer')
            with self.assertRaises(ValueError):
                quality.producer_receipt(args, item)
            report = {'native_codegen': {'backend': 'llvm-api', 'requested_units': 16, 'actual_units': 16,
                                         'jobs': 1, 'fallback': None}}
            with patch.object(matrix.matched, 'final_artifact_scope', return_value={'before_codegen': True}), \
                 patch.object(matrix.matched, 'retained_inventory', return_value={'objects': [item]}), \
                 self.assertRaises(ValueError):
                quality.inventory(args, report, out/'candidate'/'objects', b'', 'd'*64)

    def test_complete_imports_has_distinct_schema_and_producer_policy(self):
        args = self.args()
        self.assertEqual(quality.schema(args), quality.SCHEMA)
        self.assertEqual(quality.policy_identity(args), 'closed+off')
        args.complete_imports = True
        quality.policy(args)
        self.assertEqual(quality.schema(args), 'closed-import-project-runtime-v1')
        self.assertNotEqual(quality.policy_identity(args), 'closed+off')
        self.assertEqual(quality.runtime_decision([self.runtime_case()], args)['schema'], quality.IMPORT_SCHEMA)
        args.complete_imports = 1
        with self.assertRaises(ValueError):
            quality.policy(args)
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            args = self.args(out)
            receipt = {'backend': 'llvm-api', 'pipeline': 'default<O2>', 'triple': 'x86_64-unknown-linux-gnu',
                       'llvm_version': '23.1.4', 'verify_before_after': True, 'input_kind': 'builder',
                       'object_bytes': 9, 'cache': {'state': 'miss', 'action_key': 'a'*64, 'corrupt': False}}
            archived = {'receipt': receipt, 'object_sha256': 'b'*64, 'policy': quality.policy_identity(args)}
            item = {'action_key': 'a'*64, 'object': {'sha256': 'b'*64, 'bytes': 9},
                    'producer_native_receipt': matrix.matched.blob(out, json.dumps(archived).encode())}
            quality.producer_receipt(args, item)
            args.complete_imports = True
            with self.assertRaises(ValueError):
                quality.producer_receipt(args, item)
            archived['policy'] = quality.policy_identity(args)
            item['producer_native_receipt'] = matrix.matched.blob(out, json.dumps(archived).encode())
            quality.producer_receipt(args, item)
            args.complete_imports = False
            with self.assertRaises(ValueError):
                quality.producer_receipt(args, item)

    def import_layout(self):
        items = [('define', 'main')]+[('define', f'zil.fn.{i}') for i in range(1, 16)]
        units = [{'owner': i, 'prelude': [], 'declarations': [], 'imports': [], 'definitions': [i]} for i in range(16)]
        units[0]['imports'] = [1, 2]
        units[1]['imports'] = [2]
        calls = {name: set() for _, name in items}
        calls['main'] = {'zil.fn.1'}
        calls['zil.fn.1'] = {'zil.fn.2'}
        return units, items, {'definitions': {n for _, n in items}, 'calls': calls, 'foreign': {'getchar', 'putchar'}}

    def test_complete_import_layout_needs_flag_full_closure_and_unique_ownership(self):
        units, items, model = self.import_layout()
        expected, cross, imported = quality.layout_roles(units, items, model, True)
        self.assertTrue(imported)
        self.assertEqual(expected[1], {'zil.fn.1'})
        # Both canonical cross edges can disappear after private-copy inlining.
        self.assertEqual(cross, set())
        self.assertEqual(quality.import_names(units, items)[0], {'zil.import.0.zil.fn.1', 'zil.import.0.zil.fn.2'})
        with self.assertRaises(ValueError):
            quality.layout_roles(units, items, model, False)
        for change in ('partial', 'duplicate_import', 'own_import', 'wrong_owner', 'unknown_index', 'wrong_prelude', 'missing_owner'):
            bad = copy.deepcopy(units)
            if change == 'partial': bad[0]['imports'] = [1]
            if change == 'duplicate_import': bad[0]['imports'] = [1, 2, 2]
            if change == 'own_import': bad[0]['imports'] = [0, 1, 2]
            if change == 'wrong_owner': bad[0]['owner'] = 1
            if change == 'unknown_index': bad[0]['imports'] = [1, 99]
            if change == 'wrong_prelude': bad[0]['prelude'] = [1]
            if change == 'missing_owner': bad[1]['definitions'] = []
            with self.subTest(change=change), self.assertRaises(ValueError):
                quality.layout_roles(bad, items, model, True)

    def test_surviving_import_clone_must_be_derived_owner_local_default(self):
        name = 'zil.import.0.zil.fn.1'
        model = {'definitions': {'zil.fn.1', 'zil.fn.2'}, 'foreign': {'getchar', 'putchar'}}
        data = bytearray(self.object_bytes(name))
        symbol = 65+len(b'\0'+name.encode()+b'\0')+24
        with self.assertRaises(ValueError):
            quality.object_abi(bytes(data), {'zil.fn.2'}, set(), model, {name})
        data[symbol+4] = 2  # LOCAL STT_FUNC.
        accepted = quality.object_abi(bytes(data), {'zil.fn.2'}, set(), model, {name})
        self.assertEqual(accepted['functions'][name]['bind'], 0)
        with self.assertRaises(ValueError):
            quality.object_abi(bytes(data), {'zil.fn.2'}, set(), model, {'zil.import.1.zil.fn.1'})
        data[symbol+5] = 2  # HIDDEN is outside this exact clone convention.
        with self.assertRaises(ValueError):
            quality.object_abi(bytes(data), {'zil.fn.2'}, set(), model, {name})

    def bounded_layout(self, class_cost=128, leaf_cost=2, extra_caller=False):
        first, leaf = 'zil.class.'+'a'*64, 'zil.class.'+'b'*64

        def padded(prefix, value, count):
            for index in range(count):
                prefix += f' %pad{index} = add i32 {value}, 0\n'
                value = f'%pad{index}'
            return prefix+f' ret i32 {value}\n'

        leaf_body = padded('entry:\n', '%x', leaf_cost-2)
        first_body = padded(f'entry:\n %value = call i32 @{leaf}(i32 %x)\n', '%value', class_cost-3)
        llvm = ('declare i32 @getchar() nonlazybind\ndeclare i32 @putchar(i32) nonlazybind\n'
                f'define i32 @main() {{\nentry:\n %a = call i32 @{first}(i32 1)\n'
                ' %s = call i32 @zil.fn.15(i32 %a)\n %out = call i32 @putchar(i32 %s)\n ret i32 0\n}\n')
        for index in range(1, 16):
            body = first_body if index == 1 else 'entry:\n ret i32 %x\n'
            if index == 2:
                body = leaf_body
            if index == 3 and extra_caller:
                body = f'entry:\n %a = call i32 @{first}(i32 %x)\n ret i32 %a\n'
            llvm += f'define i32 @zil.fn.{index}(i32 %x) {{\n'+body+'}\n'
        llvm += f'define i32 @{first}(i32 %x) {{\n'+first_body+'}\n'
        llvm += f'define i32 @{leaf}(i32 %x) {{\n'+leaf_body+'}\n'
        llvm = llvm.encode()
        model = quality.scalar_model(llvm)
        items = [('declare', 'getchar'), ('declare', 'putchar'), ('define', 'main')]
        items += [('define', f'zil.fn.{index}') for index in range(1, 16)]
        items += [('define', first), ('define', leaf)]
        units = [dict(owner=index, definitions=[index+2], imports=[], prelude=[], declarations=[])
                 for index in range(16)]
        units[1]['definitions'].append(18)
        units[2]['definitions'].append(19)
        units[0]['prelude'] = [1]
        units[0]['declarations'] = [17]
        if class_cost <= 128:
            units[0]['imports'] = [18]
            if leaf_cost <= 128:
                units[0]['imports'].append(19)
            else:
                units[0]['declarations'].append(19)
        else:
            units[0]['declarations'].append(18)
        if leaf_cost <= 128:
            units[1]['imports'] = [19]
        else:
            units[1]['declarations'] = [19]
        if extra_caller:
            if class_cost <= 128:
                units[3]['imports'] = [18]
                if leaf_cost <= 128:
                    units[3]['imports'].append(19)
                else:
                    units[3]['declarations'] = [19]
            else:
                units[3]['declarations'] = [18]
        return units, items, model, llvm

    def test_bounded_policy_is_explicit_distinct_and_candidate_only(self):
        args = self.args()
        args.complete_imports = True
        args.bounded_class_imports = True
        quality.policy(args)
        self.assertEqual(quality.schema(args), quality.BOUNDED_SCHEMA)
        self.assertEqual(quality.policy_identity(args), 'closed+off+bounded-classes-128-v3')
        self.assertEqual(quality.runtime_decision([self.runtime_case()], args)['schema'], quality.BOUNDED_SCHEMA)
        for arm in ('baseline', 'candidate'):
            command = matrix.zil_command(args, arm, Path('/src/main.zil'), Path('/work')/arm)
            self.assertEqual('--bounded-class-imports' in command, arm == 'candidate')
            self.assertEqual('--incremental-o2' in command, arm == 'candidate')
        args.complete_imports = False
        with self.assertRaises(ValueError):
            quality.policy(args)
        args.complete_imports = True
        args.bounded_class_imports = 1
        with self.assertRaises(ValueError):
            quality.policy(args)
        args.bounded_class_imports = False
        self.assertEqual(quality.schema(args), quality.IMPORT_SCHEMA)
        self.assertEqual(quality.policy_identity(args), 'closed+off+complete-imports')

    def test_bounded_layout_class_closure_exact_cost_and_original_complete_policy(self):
        units, items, model, _ = self.bounded_layout()
        first, leaf = items[18][1], items[19][1]
        self.assertEqual(model['function_costs']['zil.fn.1'], 128)
        self.assertEqual(model['function_costs'][first], 128)
        self.assertEqual(model['function_costs'][leaf], 2)
        expected, cross, imported = quality.layout_roles(units, items, model, True, True)
        self.assertTrue(imported)
        self.assertEqual(expected[1], {'zil.fn.1', first})
        self.assertEqual(cross, {'zil.fn.15'})
        # Complete still requires the entire transitive closure, including the
        # ordinary source callee. Bounded must reject exactly that layout.
        with self.assertRaises(ValueError):
            quality.layout_roles(units, items, model, True)
        complete = copy.deepcopy(units)
        complete[0]['imports'] = [17, 18, 19]
        complete[0]['declarations'] = []
        self.assertEqual(quality.layout_roles(complete, items, model, True)[1], set())
        with self.assertRaises(ValueError):
            quality.layout_roles(complete, items, model, True, True)
        large, items, model, _ = self.bounded_layout(class_cost=129)
        self.assertEqual(model['function_costs'][first], 129)
        self.assertEqual(large[0]['imports'], [])
        self.assertIn(18, large[0]['declarations'])
        self.assertIn(first, quality.layout_roles(large, items, model, True, True)[1])

    def test_bounded_layout_refuses_wrong_closure_cost_roles_and_copy_budget(self):
        units, items, model, _ = self.bounded_layout()
        for change in ('missing', 'recursive', 'source', 'prelude', 'declaration', 'extra_role', 'cost'):
            bad, current = copy.deepcopy(units), copy.deepcopy(model)
            if change == 'missing': bad[0]['imports'] = []
            if change == 'recursive': bad[0]['imports'] = [18]
            if change == 'source': bad[0]['imports'] = [17, 18]
            if change == 'prelude': bad[0]['prelude'] = []
            if change == 'declaration': bad[0]['declarations'] = []
            if change == 'extra_role': bad[0]['prelude'] = [0, 1]
            if change == 'cost': current['function_costs'][items[18][1]] = 129
            with self.subTest(change=change), self.assertRaises(ValueError):
                quality.layout_roles(bad, items, current, True, True)
        over, items, model, _ = self.bounded_layout(extra_caller=True)
        with self.assertRaisesRegex(ValueError, 'source-only total copy cost'):
            quality.layout_roles(over, items, model, True, True)
        # Empty imports must still validate all current physical declaration
        # roles; no has_imports shortcut may admit an incomplete bounded plan.
        empty, items, model, _ = self.bounded_layout(class_cost=129, leaf_cost=129)
        self.assertFalse(quality.layout_roles(empty, items, model, True, True)[2])
        empty[0]['declarations'] = [17]
        with self.assertRaises(ValueError):
            quality.layout_roles(empty, items, model, True, True)

    def test_bounded_inventory_and_producer_require_exact_report_scope(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            args = self.args(out)
            args.complete_imports = args.bounded_class_imports = True
            units, _, _, llvm = self.bounded_layout()
            cg = dict(class_import_requested=quality.BOUNDED_POLICY, class_import_policy=quality.BOUNDED_POLICY,
                      class_import_fallback=None, fallback=None, partition_layout_key='b'*64)
            report = {'native_codegen': cg}
            inv = dict(compiler_summary=copy.deepcopy(cg), policy_identity=quality.policy_identity(args),
                       partition_layout_key='b'*64, current_layout_matches=['b'*64],
                       partition_layouts=[self.archived_layout(out, units, 'b'*64)])
            quality.current_inventory_layout(args, report, inv, llvm)
            for field, value in (('class_import_requested', None), ('class_import_policy', 'complete'),
                                 ('class_import_fallback', 'noeligibleclasses'), ('fallback', 'partition refused')):
                bad = copy.deepcopy(report)
                bad['native_codegen'][field] = value
                with self.subTest(field=field), self.assertRaises(ValueError):
                    quality.current_inventory_layout(args, bad, inv, llvm)
            for field in ('class_import_requested', 'class_import_policy', 'class_import_fallback', 'fallback'):
                bad = copy.deepcopy(report)
                del bad['native_codegen'][field]
                with self.subTest(missing=field), self.assertRaises(ValueError):
                    quality.current_inventory_layout(args, bad, inv, llvm)
            bad = copy.deepcopy(inv)
            bad['policy_identity'] = 'closed+off+complete-imports'
            with self.assertRaises(ValueError):
                quality.current_inventory_layout(args, report, bad, llvm)
            bad = copy.deepcopy(inv)
            bad['compiler_summary']['class_import_policy'] = 'complete'
            with self.assertRaises(ValueError):
                quality.current_inventory_layout(args, report, bad, llvm)
            # The baseline arm keeps its original layout/report obligations.
            baseline = dict(policy_identity='closed+off', compiler_summary={'partition_layout_key': 'b'*64},
                            partition_layouts=[self.archived_layout(out, units, 'b'*64)], current_layout_matches=['b'*64])
            # Bounded roles differ from Complete even on a baseline read; use
            # its real Complete layout rather than relaxing either policy.
            complete = copy.deepcopy(units)
            complete[0].update(imports=[17, 18, 19], declarations=[])
            baseline['partition_layouts'] = [self.archived_layout(out, complete, 'b'*64)]
            quality.current_inventory_layout(args, {'native_codegen': baseline['compiler_summary']}, baseline,
                                             llvm, candidate=False)
            receipt = {'backend': 'llvm-api', 'pipeline': 'default<O2>', 'triple': 'x86_64-unknown-linux-gnu',
                       'llvm_version': '23.1.4', 'verify_before_after': True, 'input_kind': 'builder',
                       'object_bytes': 9, 'cache': {'state': 'miss', 'action_key': 'a'*64, 'corrupt': False}}
            archived = dict(receipt=receipt, object_sha256='c'*64, policy=quality.policy_identity(args),
                            class_import_scope=copy.deepcopy(cg))
            item = dict(action_key='a'*64, object={'sha256': 'c'*64, 'bytes': 9})
            item['producer_native_receipt'] = matrix.matched.blob(out, json.dumps(archived).encode())
            quality.producer_receipt(args, item)
            for scope in (None, dict(cg, class_import_requested='complete'),
                          dict(cg, class_import_policy='complete', class_import_fallback='bounded plan refused')):
                bad = dict(archived, class_import_scope=scope)
                item['producer_native_receipt'] = matrix.matched.blob(out, json.dumps(bad).encode())
                with self.subTest(scope=scope), self.assertRaises(ValueError):
                    quality.producer_receipt(args, item)
            # A Complete producer cannot be relabeled by retaining its object.
            bad = dict(archived, policy='closed+off+complete-imports')
            item['producer_native_receipt'] = matrix.matched.blob(out, json.dumps(bad).encode())
            with self.assertRaises(ValueError):
                quality.producer_receipt(args, item)

    def test_scalar_scope_refuses_globals_callbacks_and_unknown_foreign(self):
        source = ('declare i32 @getchar() nonlazybind\ndeclare i32 @putchar(i32) nonlazybind\n'
                  'define i32 @main() {\n %x = call i32 @getchar()\n ret i32 %x\n}\n')
        quality.scalar_model(source.encode())
        for bad in (source+'@address = global i32 0\n', source.replace('@getchar()', '%callback()', 1),
                    source.replace('call i32 @getchar()', 'call i32 %callback()'),
                    source+'declare i32 @unknown_foreign()\n'):
            with self.assertRaises(ValueError):
                quality.scalar_model(bad.encode())

    def test_stable_classes_need_full_current_source_bodies_and_pure_acyclic_calls(self):
        name = 'zil.class.'+'a'*64
        header = ('declare i32 @getchar() nonlazybind\n'
                  'declare i32 @putchar(i32) nonlazybind\n'
                  'define i32 @main() {\n ret i32 0\n}\n')
        body = '() {\n ret i32 23\n}\n'
        source = header+'define i32 @zil.fn.1'+body+'define i32 @'+name+body
        model = quality.scalar_model(source.encode())
        self.assertEqual(model['classes'], {name})
        self.assertEqual(model['class_sources'], {name: ['zil.fn.1']})
        bads = [source.replace('ret i32 23', 'ret i32 24', 1),
                source.replace('define i32 @'+name, 'define void @'+name),
                source.replace('define i32 @zil.fn.1', 'define i32 @unrelated'),
                source.replace(name, 'zil.class.not-a-receipt')]
        for bad_body in ('() {\n %x = load i32, ptr null\n ret i32 %x\n}\n',
                         '() {\n ret i32 poison\n}\n'):
            bads.append(header+'define i32 @zil.fn.1'+bad_body+'define i32 @'+name+bad_body)
        for target in ('getchar', name, 'zil.fn.1'):
            called = '() {\n %x = call i32 @'+target+'()\n ret i32 %x\n}\n'
            bads.append(header+'define i32 @zil.fn.1'+called+'define i32 @'+name+called)
        for bad in bads:
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                quality.scalar_model(bad.encode())

    def test_full_import_closure_accepts_only_validated_class_members(self):
        name = 'zil.class.'+'b'*64
        items = [('define', 'main'), ('define', name)]
        units = [dict(owner=i, definitions=([i] if i < 2 else []),
                      imports=([1] if i == 0 else []), prelude=[], declarations=[])
                 for i in range(16)]
        model = dict(calls={'main': {name}, name: set()}, classes={name})
        expected, cross, imported = quality.layout_roles(units, items, model, True)
        self.assertEqual(expected[1], {name})
        self.assertEqual(cross, set())
        self.assertTrue(imported)
        with self.assertRaises(ValueError):
            quality.layout_roles(units, items, dict(model, classes=set()), True)
        units[0]['imports'] = []
        units[2]['imports'] = [1]
        with self.assertRaises(ValueError):
            quality.layout_roles(units, items, model, True)

    def test_physical_entry_and_cross_symbol_bindings_are_not_just_names(self):
        model = {'definitions': {'main', 'zil.fn.1'}, 'foreign': {'getchar', 'putchar'}}
        for name in ('main', 'zil.fn.1'):
            data = self.object_bytes(name)
            quality.object_abi(data, {name}, {name} if name != 'main' else set(), model)
            # Same spelling and bytes; LOCAL or HIDDEN cannot bind another CGU.
            symbol_offset = 65+len(b'\0'+name.encode()+b'\0')+24
            for field, value in ((4, 2), (5, 2)):
                bad = bytearray(data)
                bad[symbol_offset+field] = value
                with self.assertRaises(ValueError):
                    quality.object_abi(bytes(bad), {name}, {name} if name != 'main' else set(), model)
        with self.assertRaises(ValueError):
            quality.elf_table(self.object_bytes('main')[:70])

    def test_readonly_machine_constant_pool_does_not_admit_writable_data(self):
        symbol = dict(name='.LCPI0_0', kind=0, bind=0, visibility=0, section=1, address=0, size=0)
        readonly = (0, 1, 18, 0, 0, 16, 0, 0, 16, 16)
        table = {'symbols': [symbol], 'sections': [(0,)*10, readonly],
                 'relocations': [dict(symbol=symbol, kind=2, offset=0, addend=-4)]}
        model = {'definitions': set(), 'foreign': set()}
        with patch.object(quality, 'elf_table', return_value=table):
            quality.object_abi(b'', set(), set(), model)
        for change in ('writable', 'unknown_name', 'outside_entry', 'address_relocation'):
            bad = copy.deepcopy(table)
            if change == 'writable':
                section = list(readonly); section[2] |= 1; bad['sections'][1] = tuple(section)
            elif change == 'unknown_name':
                bad['symbols'][0]['name'] = 'user_global'
            elif change == 'outside_entry':
                bad['symbols'][0]['address'] = 16
            else:
                bad['relocations'][0]['kind'] = 1
            with self.subTest(change=change), patch.object(quality, 'elf_table', return_value=bad), self.assertRaises(ValueError):
                quality.object_abi(b'', set(), set(), model)

    def oracle(self):
        return types.SimpleNamespace(oracle=lambda case, data, label: {
            'exit': 0, 'stdout_hex': bytes([data[0] if data else 0]).hex(), 'stderr_hex': ''})

    def outcomes(self, oracle):
        return [{'stdin_hex': data.hex(), 'valid': True, 'actual': oracle.oracle('scalar', data, ''),
                 'expected': oracle.oracle('scalar', data, '')} for data in quality.INPUTS]

    def test_exhaustive_oracle_rejects_missing_duplicate_and_forged_expected(self):
        oracle = self.oracle()
        outcomes = self.outcomes(oracle)
        quality.checked_oracles({'native': outcomes}, oracle, 1)
        for bad in (outcomes[:-1], outcomes+[outcomes[0]], [outcomes[0]]+outcomes[2:]):
            with self.assertRaises(ValueError):
                quality.checked_oracles({'native': bad}, oracle, 1)
        bad = copy.deepcopy(outcomes)
        bad[-1]['actual'] = bad[-1]['expected'] = {'exit': 0, 'stdout_hex': '00', 'stderr_hex': ''}
        with self.assertRaises(ValueError):
            quality.checked_oracles({'native': bad}, oracle, 1)

    def triplet(self, out):
        oracle = self.oracle()
        report = {'status': 'Success', 'phase': 'link', 'diagnostics': [], 'verification_runs': 1,
                  'query_events': [{'phase': 'verify', 'disposition': 'executed'}],
                  'input_fingerprint': 'same', 'source_files': {'0': '/src/main.zil'}, 'scalar_interfaces': []}
        rows = []
        for arm in matrix.matched.ARMS:
            current = copy.deepcopy(report)
            key = ('a' if arm == 'baseline' else 'b')*64
            current['native_codegen'] = {'actual_units': 16, 'partition_layout_key': None if arm == 'baseline' else key}
            llvm = ('declare i32 @getchar() nonlazybind\ndeclare i32 @putchar(i32) nonlazybind\n'
                    'define i32 @main() {\n ret i32 0\n}\n'
                    'define i32 @zil.fn.0() {\n ret i32 23\n}\n').encode()
            units = [dict(owner=i, prelude=[], declarations=[], imports=[],
                          definitions=[2, 3] if i == 0 else []) for i in range(16)]
            layout = self.archived_layout(out, units, key)
            functions = ['main', 'zil.fn.0'] if arm == 'baseline' else ['main']
            objects = [{'defined_functions': functions}]+[{'defined_functions': []} for _ in range(15)]
            inv = {'objects': objects, 'object_count': 16, 'current_layout_matches': [key],
                   'partition_layouts': [layout], 'compiler_summary': current['native_codegen'],
                   'removed_functions': [] if arm == 'baseline' else ['zil.fn.0']}
            rows.append({'arm': arm, 'request': 'cold', 'valid': True, 'resident': {'report': current},
                         'llvm': matrix.matched.blob(out, llvm, '.ll'),
                         'executable': matrix.matched.blob(out, b'\x7fELF'+arm.encode(), '.elf'),
                         'inventory': inv, 'native': self.outcomes(oracle)})
        return rows, oracle

    def layout_bytes(self, units, key):
        payload = len(units).to_bytes(4, 'little')
        for unit in units:
            payload += unit['owner'].to_bytes(4, 'little')
            for group in ('prelude', 'declarations', 'imports', 'definitions'):
                payload += len(unit[group]).to_bytes(4, 'little')
                payload += b''.join(i.to_bytes(4, 'little') for i in unit[group])
        return b'ZILCGU01'+bytes.fromhex(key)+bytes.fromhex(matrix.rc.sha(payload))+payload

    def archived_layout(self, out, units, key):
        data = self.layout_bytes(units, key)
        return dict(key=key, units=copy.deepcopy(units), payload_sha256=matrix.rc.sha(data[72:]),
                    **matrix.matched.blob(out, data, '.layout'))

    def aggregate_abi_stub(self, args, row):
        # Physical executable ABI is tested separately; keep the actual archived
        # model/layout binding live in these aggregate-only fixtures.
        llvm = matrix.matched.recheck_archived(args.out, row['llvm'])
        model = quality.scalar_model(llvm)
        layout, _ = quality.current_inventory_layout(args, row['resident']['report'], row['inventory'], llvm, model)
        return {'scope': 'mock physical executable ABI; actual layout/model rechecked',
                'class_sources': model['class_sources'], 'partition_layout_key': layout['key']}

    def test_new_protocol_records_unequal_elf_and_requires_current_verify(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            rows, oracle = self.triplet(out)
            with patch.object(quality, 'executable_abi', side_effect=self.aggregate_abi_stub):
                result = quality.quality(rows, self.args(out), oracle)
            self.assertTrue(result['valid'])
            self.assertFalse(result['complete_ELF_equal'])
            self.assertTrue(result['runtime_acceptance_pending'])
            rows[1]['resident']['report']['verification_runs'] = 0
            with self.assertRaises(ValueError):
                quality.quality(rows, self.args(out), oracle)
            self.assertIn('identical complete ELF', matrix.matched.QUALITY_POLICY['machine'])

    def test_aggregate_accounts_for_only_full_body_validated_new_classes(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            rows, oracle = self.triplet(out)
            live, dead = 'zil.class.'+'a'*64, 'zil.class.'+'b'*64
            rows[1]['inventory']['objects'][0]['defined_functions'].append(live)
            rows[1]['inventory']['removed_functions'].append(dead)
            llvm = matrix.matched.recheck_archived(out, rows[1]['llvm'])
            llvm += ''.join(f'define i32 @{name}() {{\n ret i32 23\n}}\n' for name in (live, dead)).encode()
            rows[1]['llvm'] = matrix.matched.blob(out, llvm, '.ll')
            units = copy.deepcopy(rows[1]['inventory']['partition_layouts'][0]['units'])
            units[0]['definitions'].extend([4, 5])
            rows[1]['inventory']['partition_layouts'] = [self.archived_layout(out, units, 'b'*64)]
            with patch.object(quality, 'executable_abi', side_effect=self.aggregate_abi_stub):
                self.assertTrue(quality.quality(rows, self.args(out), oracle)['valid'])
                for mutate in ('unvalidated_extra', 'missing_source_removal', 'live_claimed_removed'):
                    bad = copy.deepcopy(rows)
                    if mutate == 'unvalidated_extra':
                        bad[1]['inventory']['objects'][0]['defined_functions'].append('zil.class.'+'c'*64)
                    elif mutate == 'missing_source_removal':
                        bad[1]['inventory']['removed_functions'].remove('zil.fn.0')
                    else:
                        bad[1]['inventory']['removed_functions'].append(live)
                    with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                        quality.quality(bad, self.args(out), oracle)
                for key in (None, 'not-a-key', 'c'*64):
                    bad = copy.deepcopy(rows)
                    bad[1]['resident']['report']['native_codegen']['partition_layout_key'] = key
                    with self.subTest(key=key), self.assertRaises(ValueError):
                        quality.quality(bad, self.args(out), oracle)
                bad = copy.deepcopy(rows)
                actual = self.archived_layout(out, units, 'c'*64)
                # All prefilled metadata claims the report key, but the actual
                # content-addressed archive still contains a different key.
                actual['key'] = 'b'*64
                bad[1]['inventory']['partition_layouts'] = [actual]
                with self.assertRaises(ValueError):
                    quality.quality(bad, self.args(out), oracle)

    def test_main_resource_failure_is_explicit_and_never_a_ratio(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            rows, oracle = self.triplet(out)
            rows[0] = {'arm': 'baseline', 'request': 'cold', 'valid': False,
                'resident': {'report': {'status': 'ResourceLimit', 'phase': 'expand', 'verification_runs': 0,
                    'diagnostics': [{'category': 'ResourceLimit'}]}},
                'publication_before': {'program': {'exists': False}},
                'publication_after': {'program': {'exists': False}}}
            with patch.object(quality, 'executable_abi', side_effect=self.aggregate_abi_stub):
                result = quality.quality(rows, self.args(out), oracle, allow_main_limit=True)
            self.assertTrue(result['main_resource_limit'])
            self.assertFalse(result['main_success'])
            self.assertIsNone(result['complete_ELF_equal'])
            samples = [{'arm': arm, 'valid': True, 'block': b, 'measurement': {'wall_ms': value}}
                       for b in range(2) for arm, value in (('candidate', 2), ('rust', 3))]
            comparison = quality.paired_comparison(samples, 2, ('candidate', 'rust'))
            self.assertNotIn('candidate_over_baseline_paired_GM', comparison)
            rows[0]['resident']['report']['status'] = 'Unsupported'
            with self.assertRaises(ValueError):
                quality.quality(rows, self.args(out), oracle, allow_main_limit=True)

    def runtime_case(self, main=True, candidate=100, rust=100):
        values = {'candidate': candidate, 'rust': rust}
        if main:
            values['baseline'] = 100
        return {'case': '1000/cold', 'main_resource_limit': not main,
                'samples': {arm: [{'valid': True, 'timed_out': False, 'wall_ms': value, 'wait4_maxrss_kib': 1000}
                                  for _ in range(6)] for arm, value in values.items()}}

    def test_runtime_limits_and_missing_main_never_invent_pass(self):
        self.assertTrue(quality.runtime_decision([self.runtime_case()])['accepted'])
        self.assertFalse(quality.runtime_decision([self.runtime_case(candidate=111, rust=111)])['accepted'])
        self.assertFalse(quality.runtime_decision([self.runtime_case(rust=89)])['candidate_rust_accepted'])
        result = quality.runtime_decision([self.runtime_case(main=False)])
        self.assertTrue(result['candidate_rust_accepted'])
        self.assertIsNone(result['main_regression_accepted'])
        self.assertFalse(result['accepted'])
        self.assertIsNone(result['cases'][0]['candidate_over_main'])
        bad = self.runtime_case()
        bad['samples']['rust'].pop()
        with self.assertRaises(ValueError):
            quality.runtime_decision([bad])

    def test_periodic_wrapper_oracle_uses_every_seed(self):
        args = self.args()
        oracle = matrix.Oracle(args, None, {'name': 'cold'}, sustained=True)
        oracle.scalar = lambda data: {'stdout_hex': int(data[0]+1).to_bytes(4, 'little').hex()}
        for seed in (0, 65, 250):
            expected = sum((seed+i) % 251+1 for i in range(10000)) % 1000003
            actual = oracle.oracle('scalar', bytes([seed]), '')
            self.assertEqual(bytes.fromhex(actual['stdout_hex']), expected.to_bytes(4, 'little'))

    def test_pack_keeps_every_raw_byte_with_deduplication(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            leaf = out/'native'
            leaf.mkdir()
            (leaf/'stdout.bin').write_bytes(b'abc')
            (leaf/'stderr.bin').write_bytes(b'')
            packed = matrix.packed_directory(out, leaf)
            listing = json.loads(matrix.matched.recheck_archived(out, packed))
            self.assertEqual(matrix.matched.recheck_archived(out, listing['stdout.bin']), b'abc')
            self.assertEqual(matrix.matched.recheck_archived(out, listing['stderr.bin']), b'')
            self.assertFalse(leaf.exists())

    def test_parallel_native_validation_preserves_257_order_failures_and_all_raw(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            args = self.args(out)
            args.complete_imports = True
            args.native_validation_jobs = 8
            args.native_timeout, args.sample_ms = 10, 5
            program, directory = out/'program', out/'request'
            program.write_bytes(b'\x7fELFmock-only-never-executed')
            directory.mkdir()
            oracle = self.oracle()
            oracle.inputs = lambda case: quality.INPUTS
            completed, packed = set(), []
            lock = threading.Lock()
            original_pack = matrix.packed_directory

            def run(command, cwd, leaf, timeout, sample_ms, stdin=b'', env=None):
                leaf.mkdir()
                expected = oracle.oracle('scalar', stdin, 'cold')
                stdout = b'wrong' if stdin == b'\xff' else bytes.fromhex(expected['stdout_hex'])
                stderr = b'timeout-marker' if stdin == b'\x80' else b''
                result = {'returncode': 0, 'timed_out': stdin == b'\x80', 'wall_ms': 1.0}
                files = {'stdin.bin': stdin, 'stdout.bin': stdout, 'stderr.bin': stderr,
                         'command.json': json.dumps({'argv': command, 'cwd': str(cwd)}).encode(),
                         'result.json': json.dumps(result).encode(), 'memory-samples.json': b'[]'}
                for name, data in files.items():
                    (leaf/name).write_bytes(data)
                with lock:
                    self.assertNotIn(leaf, completed)
                    completed.add(leaf)
                return result

            def pack(root, leaf):
                # Packing must occur serially, after all native workers joined.
                self.assertEqual(threading.current_thread(), threading.main_thread())
                with lock:
                    self.assertEqual(len(completed), 257)
                packed.append(leaf)
                return original_pack(root, leaf)

            with patch.object(matrix.rc, 'run_process', side_effect=run) as runner, \
                 patch.object(matrix, 'packed_directory', side_effect=pack):
                outcomes = matrix.native_outcomes(args, oracle, program, 'cold', directory, {'LC_ALL': 'C'})
            self.assertEqual(runner.call_count, 257)
            self.assertEqual(len(completed), 257)
            self.assertEqual(len(packed), 257)
            self.assertEqual([o['stdin_hex'] for o in outcomes], [data.hex() for data in quality.INPUTS])
            self.assertEqual([o['stdin_hex'] for o in outcomes if not o['valid']], ['80', 'ff'])
            for data, outcome in zip(quality.INPUTS, outcomes):
                listing = json.loads(matrix.matched.recheck_archived(out, outcome['raw']))
                self.assertEqual(set(listing), {'stdin.bin', 'stdout.bin', 'stderr.bin', 'command.json', 'result.json', 'memory-samples.json'})
                raw = {name: matrix.matched.recheck_archived(out, reference) for name, reference in listing.items()}
                self.assertEqual(raw['stdin.bin'], data)
                self.assertEqual(raw['stdout.bin'].hex(), outcome['actual']['stdout_hex'])
                self.assertEqual(raw['stderr.bin'].hex(), outcome['actual']['stderr_hex'])
                self.assertEqual(json.loads(raw['command.json'])['argv'], [str(program)])
                self.assertEqual(json.loads(raw['result.json'])['timed_out'], data == b'\x80')
                self.assertEqual(raw['memory-samples.json'], b'[]')
            self.assertFalse(any(directory.glob('native-*')))

    def object_bytes(self, name):
        header = bytearray(64)
        header[:7] = b'\x7fELF\x02\x01\x01'
        struct.pack_into('<HHI', header, 16, 1, 62, 1)
        struct.pack_into('<H', header, 52, 64)
        text = b'\xc3'
        strings = b'\0'+name.encode()+b'\0'
        symbols = bytes(24)+struct.pack('<IBBHQQ', 1, 2, 0, 1, 0, 1)
        # GLOBAL STT_FUNC; .text, .strtab, .symtab are real separate sections.
        symbols = bytes(24)+struct.pack('<IBBHQQ', 1, 0x12, 0, 1, 0, 1)
        offset = 65+len(strings)+len(symbols)
        struct.pack_into('<Q', header, 40, offset)
        struct.pack_into('<HH', header, 58, 64, 4)
        sections = bytes(64)+struct.pack('<IIQQQQIIQQ', 0, 1, 6, 0, 64, 1, 0, 0, 1, 0)
        sections += struct.pack('<IIQQQQIIQQ', 0, 3, 0, 0, 65, len(strings), 0, 0, 1, 0)
        sections += struct.pack('<IIQQQQIIQQ', 0, 2, 0, 0, 65+len(strings), len(symbols), 2, 0, 8, 24)
        return bytes(header)+text+strings+symbols+sections

    def test_physical_closed_cgus_bind_layout_and_reject_object_swap(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            cache = out/'candidate'/'objects'
            namespace = cache/'objects-v1'/('a'*64)
            namespace.mkdir(parents=True)
            reports, payload = [], (16).to_bytes(4, 'little')
            for i in range(16):
                key = f'{i+1:064x}'
                obj = self.object_bytes('main' if i == 0 else 'zil.fn.'+str(i))
                (namespace/(key+'.zobj')).write_bytes(b'ZILOBJ01'+bytes.fromhex(key)+bytes.fromhex(matrix.rc.sha(obj))+len(obj).to_bytes(8, 'little')+obj)
                reports.append({'backend': 'llvm-api', 'pipeline': 'default<O2>', 'verify_before_after': True,
                                'triple': 'x86_64-unknown-linux-gnu', 'llvm_version': '23.1.4', 'input_kind': 'builder',
                                'object_bytes': len(obj), 'cache': {'state': 'miss', 'action_key': key, 'corrupt': False, 'current_preflight_ns': 0}})
                payload += i.to_bytes(4, 'little')+bytes(12)+(1).to_bytes(4, 'little')+(i+2).to_bytes(4, 'little')
            layouts = cache/'partition-layout-v1'
            layouts.mkdir()
            (layouts/('b'*64+'.layout')).write_bytes(b'ZILCGU01'+bytes.fromhex('b'*64)+bytes.fromhex(matrix.rc.sha(payload))+payload)
            report = {'native_codegen': {'backend': 'llvm-api', 'requested_units': 16, 'actual_units': 16,
                'jobs': 1, 'fallback': None, 'object_reports': reports,
                # Completed graph statistics alone do not define a class domain.
                'call_classes': {'equivalence_classes': 0},
                'object_cache': {'hits': 0, 'misses': 16, 'corrupt': 0, 'write_failures': 0,
                                 'current_preflight_ns': 0, 'object_hit_preflight_checked_units': 0, 'object_hit_preflight_reused_units': 0}}}
            llvm = ('; ZIL verified LLVM module\n; verified-fingerprint: '+'0'*64+'\n'
                    'declare i32 @getchar() nonlazybind\ndeclare i32 @putchar(i32) nonlazybind\n'
                    'define i32 @main() {\n ret i32 0\n}\n'+''.join(f'define void @zil.fn.{i}() {{\n ret void\n}}\n' for i in range(1, 16))).encode()
            def symbols(args, item):
                blob = matrix.matched.blob(args.out, b'symbols'+item['sha256'].encode())
                return {'path': blob['archive'], 'sha256': blob['sha256']}
            with patch.object(matrix.matched, 'symbols', symbols):
                # A previous valid envelope can describe an older call graph.
                # It sorts first here but must never become current provenance.
                prior = matrix.matched.layout_units((layouts/('b'*64+'.layout')).read_bytes())
                prior[0]['imports'] = [3]
                stale = (16).to_bytes(4, 'little')
                for unit in prior:
                    stale += unit['owner'].to_bytes(4, 'little')
                    for group in ('prelude', 'declarations', 'imports', 'definitions'):
                        stale += len(unit[group]).to_bytes(4, 'little')
                        stale += b''.join(i.to_bytes(4, 'little') for i in unit[group])
                (layouts/('a'*64+'.layout')).write_bytes(b'ZILCGU01'+bytes.fromhex('a'*64)+bytes.fromhex(matrix.rc.sha(stale))+stale)
                imports_args = self.args(out)
                imports_args.complete_imports = True
                current = quality.inventory(imports_args, report, cache, llvm, 'c'*64)
                self.assertEqual(current['object_count'], 16)
                good = layouts/('b'*64+'.layout')
                saved = good.read_bytes()
                good.unlink()
                with self.assertRaises(ValueError):
                    quality.inventory(imports_args, report, cache, llvm, 'c'*64)
                good.write_bytes(saved)
                inv = quality.inventory(self.args(out), report, cache, llvm, 'c'*64)
                self.assertEqual(inv['object_count'], 16)
                self.assertEqual(inv['removed_functions'], [])
                # Two different layouts can both satisfy current IR/ELF after
                # O2 removes a pure class. Only the real report key selects one.
                class_name = 'zil.class.'+'d'*64
                class_llvm = llvm.replace(b'define void @zil.fn.1() {\n ret void\n}',
                                         b'define i32 @zil.fn.1() {\n ret i32 7\n}')
                class_llvm += f'define i32 @{class_name}() {{\n ret i32 7\n}}\n'.encode()
                current_units = matrix.matched.layout_units(saved)
                current_units[0]['definitions'].append(18)
                prior_units = copy.deepcopy(current_units)
                prior_units[0]['definitions'].remove(18)
                prior_units[1]['definitions'].append(18)
                good.write_bytes(self.layout_bytes(current_units, 'b'*64))
                (layouts/('a'*64+'.layout')).write_bytes(self.layout_bytes(prior_units, 'a'*64))
                report['native_codegen']['partition_layout_key'] = 'b'*64
                current = quality.inventory(imports_args, report, cache, class_llvm, 'c'*64)
                self.assertEqual(current['current_layout_matches'], ['b'*64])
                self.assertEqual([layout['key'] for layout in current['partition_layouts']], ['b'*64])
                self.assertEqual(current['partition_layouts'][0]['units'], current_units)
                self.assertEqual(current['removed_functions'], [class_name])
                report['native_codegen']['partition_layout_key'] = 'a'*64
                prior = quality.inventory(imports_args, report, cache, class_llvm, 'c'*64)
                self.assertEqual(prior['partition_layouts'][0]['units'], prior_units)
                self.assertNotEqual(prior_units, current_units)
                # The latest unkeyed IR+ELF history is now A; terminal reuse of
                # B must restore B's actual archived producer, not relabel A.
                retained_report = copy.deepcopy(report)
                retained_report.update(commands=[], backend_work={'reused_link': True})
                retained_cg = retained_report['native_codegen']
                retained_cg['partition_layout_key'] = 'b'*64
                retained_cg['object_reports'] = []
                retained_cg['object_cache'].update(hits=0, misses=0, stored=0)
                retained_cg['final_artifact'] = dict(reused_program=True, retained_files=True,
                    before_codegen=True, profile_id=imports_args.native_link_profile_sha256,
                    current_preflight_checked_units=0, current_preflight_reused_units=16,
                    llvm_verification_scope='retained-producer', current_llvm_verifier_executed=False)
                retained = quality.inventory(imports_args, retained_report, cache, class_llvm, 'c'*64)
                self.assertEqual(retained['partition_layouts'][0]['key'], 'b'*64)
                self.assertEqual(retained['partition_layouts'][0]['units'], current_units)
                self.assertEqual(retained['current_object_report_count'], 0)
                # A new dead-class diagnostic/pre-prune layout can coexist with
                # unchanged pruned native units. Without an exact real producer
                # inventory, this harness must reject, not borrow the latest key
                # or treat an ELF/hash match as a new native producer receipt.
                new_llvm = class_llvm.replace(class_name.encode(), ('zil.class.'+'e'*64).encode())
                new_report = copy.deepcopy(retained_report)
                new_report['native_codegen']['partition_layout_key'] = 'e'*64
                (layouts/('e'*64+'.layout')).write_bytes(self.layout_bytes(current_units, 'e'*64))
                model = quality.scalar_model(new_llvm)
                items = [(m[1].decode(), m[2].decode()) for m in quality.re.finditer(
                    rb'(?m)^(declare|define) [^\n]*?@([^ (]+)\(', new_llvm)]
                quality.layout_roles(current_units, items, model, True)
                with self.assertRaisesRegex(ValueError, 'actual current layout-key producer'):
                    quality.inventory(imports_args, new_report, cache, new_llvm, 'c'*64)
                for key in (None, 'not-a-key', '../bad', 7, 'f'*64):
                    for before_codegen in (False, True):
                        bad = copy.deepcopy(retained_report if before_codegen else report)
                        bad['native_codegen']['partition_layout_key'] = key
                        with self.subTest(key=key, retained=before_codegen), self.assertRaises(ValueError):
                            quality.inventory(imports_args, bad, cache, class_llvm, 'c'*64)
                forged = copy.deepcopy(prior)
                forged.update(compiler_summary=copy.deepcopy(retained_cg),
                              partition_layout_key='b'*64, current_layout_matches=['b'*64])
                forged['partition_layouts'][0]['key'] = 'b'*64
                forged_row = dict(resident={'report': retained_report}, inventory=forged,
                                  llvm=matrix.matched.blob(out, class_llvm, '.ll'))
                with self.assertRaises(ValueError):
                    quality.executable_abi(imports_args, forged_row)
                history_key = (str(cache.resolve()), matrix.matched.pc.llvm_comparison_sha256(class_llvm), 'c'*64, 'b'*64)
                sealed = imports_args._zil_producer_inventories[history_key]
                imports_args._zil_producer_inventories[history_key] = forged
                with self.assertRaises(ValueError):
                    quality.inventory(imports_args, retained_report, cache, class_llvm, 'c'*64)
                imports_args._zil_producer_inventories[history_key] = sealed
                # Key selection still runs the unchanged whole layout/closure
                # and object ABI checks, rather than granting trust to a name.
                report['native_codegen']['partition_layout_key'] = 'b'*64
                good.write_bytes(b'corrupt selected layout')
                with self.assertRaises(ValueError):
                    quality.inventory(imports_args, report, cache, class_llvm, 'c'*64)
                wrong_closure = copy.deepcopy(current_units)
                wrong_closure[0]['imports'] = [3]
                good.write_bytes(self.layout_bytes(wrong_closure, 'b'*64))
                with self.assertRaises(ValueError):
                    quality.inventory(imports_args, report, cache, class_llvm, 'c'*64)
                good.write_bytes(self.layout_bytes(current_units, 'b'*64))
                reports[0]['cache']['action_key'], reports[1]['cache']['action_key'] = reports[1]['cache']['action_key'], reports[0]['cache']['action_key']
                with self.assertRaises(ValueError):
                    quality.inventory(imports_args, report, cache, class_llvm, 'c'*64)



class PureMachineFactsTests(unittest.TestCase):
    def test_current_pure_call_and_cfg_proof_rejects_effects_and_nontermination(self):
        attrs = ' nounwind willreturn memory(none)'
        prefix = 'declare i32 @getchar() nonlazybind\ndeclare i32 @putchar(i32) nonlazybind\n'
        main = 'define i32 @main() {\nentry:\n %x = call i32 @zil.fn.1(i32 1)\n ret i32 %x\n}\n'
        def source(body): return (prefix+main+'define i32 @zil.fn.1(i32 %x)'+attrs+' {\n'+body+'\n}\n').encode()
        quality.scalar_model(source('entry:\n %r = srem i32 %x, 251\n ret i32 %r'))
        for body in ['entry:\n %r = call i32 @putchar(i32 %x)\n ret i32 %r',
                     'entry:\n br label %entry', 'entry:\n ret i32 undef',
                     'entry:\n %r = srem i32 %x, 0\n ret i32 %r',
                     'entry:\n %r = call i32 @zil.fn.1(i32 %x)\n ret i32 %r']:
            with self.subTest(body=body), self.assertRaises(ValueError): quality.scalar_model(source(body))


if __name__ == '__main__':
    unittest.main()
