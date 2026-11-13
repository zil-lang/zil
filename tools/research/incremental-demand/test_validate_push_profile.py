#!/usr/bin/env python3
"""Small adversarial checks for the diagnostic-only profile qualifier."""
import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

DIRECTORY = Path(__file__).resolve().parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, DIRECTORY/(name+'.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


validator = load('validate_push_profile')
pc = load('push_counters')


class PushProfileQualification(unittest.TestCase):
    def arguments(self):
        return dict(case='scalar', modules=20, functions=20, steps=16, blocks=1,
                    request=None, mode='O2', backend='llvm-api', cache='all',
                    source_cache=False, closed_executable=False,
                    codegen_units=16, codegen_jobs=1, capacity=256, extra_arg=[],
                    baseline_extra_arg=[], candidate_extra_arg=['--push-reuse', '--profile-compilation'],
                    timeout=180, native_timeout=10, sample_ms=10,
                    baseline_source=validator.BASELINE, candidate_source=validator.SOURCE,
                    baseline_llvm_library_path=validator.LIBRARIES['baseline'],
                    candidate_llvm_library_path=validator.LIBRARIES['candidate'],
                    baseline='/fixed/main', candidate='/fixed/candidate')

    def test_original_protocol_and_complete_history(self):
        validator.check_arguments(self.arguments())
        history = list(validator.pairs())
        self.assertEqual((len(history), sum(p[2] for p in history)), (17, 9))
        self.assertEqual(history[-4:], [(7, 'error', False, None), (7, 'recovery', True, 'error'),
                                        (8, 'private', False, None), (8, 'restore', True, 'private')])
        self.assertEqual([p[1] for p in history if p[2]], validator.REQUESTS)

    def test_flags_cannot_change_budget_optimization_or_execution_scope(self):
        for field, wrong in [('candidate_extra_arg', ['--push-reuse']),
                             ('candidate_extra_arg', ['--push-reuse', '--profile-compilation', '--max-nodes', '0']),
                             ('extra_arg', ['--opt', 'O0']), ('baseline_extra_arg', ['--push-reuse']),
                             ('codegen_units', 1), ('codegen_jobs', 2), ('steps', 1),
                             ('blocks', 6), ('cache', 'none'), ('capacity', 4096)]:
            arguments = copy.deepcopy(self.arguments())
            arguments[field] = wrong
            with self.subTest(field=field, wrong=wrong), self.assertRaises(ValueError):
                validator.check_arguments(arguments)

    def test_full_controller_arguments_reject_unknown_or_duplicate_options(self):
        expected = validator.expected_controller_command(Path('/fixed/repo'), Path('/fixed/raw'), self.arguments())
        self.assertEqual(expected[-2:], ['--candidate-extra-arg=--push-reuse', '--candidate-extra-arg=--profile-compilation'])
        record = {'source': validator.SOURCE, 'binary_sha256': validator.CANDIDATE_BINARY, 'command': expected}
        validator.check_controller_record(record, Path('/fixed/repo'), Path('/fixed/raw'), self.arguments())
        for extra in [['--max-nodes', '0'], ['--codegen-units', '1'], ['--capacity', '256']]:
            bad = dict(record, command=expected+extra)
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                validator.check_controller_record(bad, Path('/fixed/repo'), Path('/fixed/raw'), self.arguments())
        self.assertEqual(expected.count('--codegen-units'), 1)

    def test_unknown_controller_preserves_quality_scope_without_timing_claim(self):
        self.assertIsNone(validator.parse_controller_exit('unknown'))
        self.assertEqual(validator.controller_status(None, 0),
                         {'controller_exit': None, 'persisted_helper_exit': 0,
                          'controller_agreement': None, 'timing_window_verified': False})
        self.assertEqual(validator.controller_status(0, 0),
                         {'controller_exit': 0, 'persisted_helper_exit': 0,
                          'controller_agreement': True, 'timing_window_verified': True})
        for external, persisted in [(None, 1), (0, 1), (1, 0), (True, 0)]:
            with self.subTest(external=external, persisted=persisted), self.assertRaises(ValueError):
                validator.controller_status(external, persisted)

    def test_second_frozen_source_has_its_own_exact_binary_binding(self):
        source = '328eb392d8d88148a75a743d899b43aebae73587'
        arguments = self.arguments()
        arguments['candidate_source'] = source
        validator.check_arguments(arguments, source)
        with self.assertRaises(ValueError):
            validator.check_arguments(arguments)
        command = validator.expected_controller_command(Path('/repo'), Path('/raw'), arguments)
        record = {'source': source, 'binary_sha256': validator.CANDIDATES[source], 'command': command}
        validator.check_controller_record(record, Path('/repo'), Path('/raw'), arguments, source)
        record['binary_sha256'] = validator.CANDIDATE_BINARY
        with self.assertRaises(ValueError):
            validator.check_controller_record(record, Path('/repo'), Path('/raw'), arguments, source)

    def test_inventory_and_densemap_source_has_explicit_real_binary_pin(self):
        source = '03b078b5799fe44d98855f26279f0f09e1d25bf5'
        self.assertRegex(validator.CANDIDATES[source], r'^[0-9a-f]{64}$')
        arguments = self.arguments()
        arguments['candidate_source'] = source
        validator.check_arguments(arguments, source)
        record = {'source': source, 'binary_sha256': validator.CANDIDATES[source],
                  'command': validator.expected_controller_command(Path('/repo'), Path('/raw'), arguments)}
        validator.check_controller_record(record, Path('/repo'), Path('/raw'), arguments, source)
        for old_source in [validator.SOURCE, '328eb392d8d88148a75a743d899b43aebae73587']:
            record['binary_sha256'] = validator.CANDIDATES[old_source]
            with self.subTest(old_source=old_source), self.assertRaises(ValueError):
                validator.check_controller_record(record, Path('/repo'), Path('/raw'), arguments, source)

    def test_all_old_sources_stay_bound_to_sdk15(self):
        for source in [validator.SOURCE, '328eb392d8d88148a75a743d899b43aebae73587',
                       '03b078b5799fe44d98855f26279f0f09e1d25bf5']:
            libraries, dsos = validator.runtime_pins(source)
            self.assertEqual((libraries['candidate'], dsos['candidate']),
                             ('/workspace/.zil-environment/llvm-function-products-v15/lib',
                              '200a8dbefbb8dc13dfff539a72969378c57feb7a8f2d36c635589507c4376461'))
            libraries['candidate'] = '/changed-by-caller'
            self.assertEqual(validator.runtime_pins(source)[0]['candidate'],
                             '/workspace/.zil-environment/llvm-function-products-v15/lib')
            arguments = self.arguments()
            arguments['candidate_source'] = source
            arguments['candidate_llvm_library_path'] = '/workspace/.zil-environment/llvm-function-products-v16/lib'
            with self.subTest(source=source), self.assertRaises(ValueError):
                validator.check_arguments(arguments, source)

    def test_registered_binary_without_runtime_has_no_default_fallback(self):
        source = '1'*40  # Synthetic unit-test source, never a real build pin.
        with patch.dict(validator.CANDIDATES, {source: '2'*64}):
            with self.assertRaises(ValueError):
                validator.runtime_pins(source)

    def test_new_runtime_is_isolated_from_sdk15_sources(self):
        source = '1'*40  # Synthetic unit-test source, not the pending real source.
        sdk16 = ('/workspace/.zil-environment/llvm-function-products-v16/lib',
                 '3930f308e6d9dd80dbe32fa05713335f518b883b1b24c8008d027722712c153b')
        with patch.dict(validator.CANDIDATES, {source: '2'*64}), \
             patch.dict(validator.CANDIDATE_RUNTIMES, {source: sdk16}):
            libraries, dsos = validator.runtime_pins(source)
            self.assertEqual((libraries['candidate'], dsos['candidate']), sdk16)
            arguments = self.arguments()
            arguments['candidate_source'] = source
            with self.assertRaises(ValueError):
                validator.check_arguments(arguments, source)
            arguments['candidate_llvm_library_path'] = sdk16[0]
            validator.check_arguments(arguments, source)
            self.assertEqual(validator.runtime_pins(validator.SOURCE)[1]['candidate'],
                             validator.SDK15_RUNTIME[1])

    def test_build_runtime_path_and_hash_cannot_mix_sdk_versions(self):
        libraries, dsos = validator.runtime_pins(validator.SOURCE)
        valid = {'path': libraries['candidate']+'/libLLVM.so.23.1',
                 'sha256': dsos['candidate']}
        validator.check_build_runtime(valid, libraries, dsos)
        for wrong in [dict(valid, path='/workspace/.zil-environment/llvm-function-products-v16/lib/libLLVM.so.23.1'),
                      dict(valid, sha256='3930f308e6d9dd80dbe32fa05713335f518b883b1b24c8008d027722712c153b')]:
            with self.subTest(wrong=wrong), self.assertRaises(ValueError):
                validator.check_build_runtime(wrong, libraries, dsos)

    def test_combined_backend_source_uses_only_its_sdk16_and_binary(self):
        source = '4b809bbfedb92b23062d5bf151eb75fef4bfcd35'
        self.assertEqual(validator.CANDIDATES[source],
                         '4774b015885b9de4ecfba375c2c6cd156541bde39ac814e41e960a6dead0c7f4')
        libraries, dsos = validator.runtime_pins(source)
        self.assertEqual((libraries['candidate'], dsos['candidate']),
                         ('/workspace/.zil-environment/llvm-function-products-v16/lib',
                          '3930f308e6d9dd80dbe32fa05713335f518b883b1b24c8008d027722712c153b'))
        arguments = self.arguments()
        arguments['candidate_source'] = source
        with self.assertRaises(ValueError):
            validator.check_arguments(arguments, source)
        arguments['candidate_llvm_library_path'] = libraries['candidate']
        validator.check_arguments(arguments, source)
        record = {'source': source, 'binary_sha256': validator.CANDIDATES[source],
                  'command': validator.expected_controller_command(Path('/repo'), Path('/raw'), arguments)}
        validator.check_controller_record(record, Path('/repo'), Path('/raw'), arguments, source)
        record['binary_sha256'] = validator.CANDIDATES['03b078b5799fe44d98855f26279f0f09e1d25bf5']
        with self.assertRaises(ValueError):
            validator.check_controller_record(record, Path('/repo'), Path('/raw'), arguments, source)
        for old_source in [validator.SOURCE, '328eb392d8d88148a75a743d899b43aebae73587',
                           '03b078b5799fe44d98855f26279f0f09e1d25bf5']:
            self.assertEqual(validator.runtime_pins(old_source)[1]['candidate'], validator.SDK15_RUNTIME[1])

    def test_only_second_line_fingerprint_is_normalized(self):
        prefix = b'; ZIL verified LLVM module\n; verified-fingerprint: '
        a = prefix + b'a'*64 + b'\ndefine i64 @f() { ret i64 1 }\n'
        b = prefix + b'b'*64 + b'\ndefine i64 @f() { ret i64 1 }\n'
        self.assertNotEqual(validator.sha(a), validator.sha(b))
        self.assertEqual(pc.llvm_comparison_sha256(a), pc.llvm_comparison_sha256(b))
        self.assertNotEqual(pc.llvm_comparison_sha256(a), pc.llvm_comparison_sha256(b.replace(b'i64 1', b'i64 2')))
        for malformed in [b'\n'+a, a.replace(b'\n', b'\r\n'), a+b'; verified-fingerprint: '+b'c'*64+b'\n', prefix+b'A'*64+b'\n']:
            with self.subTest(malformed=malformed[:60]), self.assertRaises(ValueError):
                pc.llvm_comparison_sha256(malformed)

    def test_sdk18_source_binary_runtime_and_complete_protocol_are_exact(self):
        source = '7bac1cc5b626594d8db36e9d765051e35b0661a2'
        self.assertEqual(validator.ELF_ARCHIVE_SOURCE, source)
        self.assertEqual(validator.CANDIDATES[source],
                         '999cedf8fa5ac06eb39c180cc0053ff9c998ab0bf72e6c0b1a6edea5dca218f6')
        libraries, dsos = validator.runtime_pins(source)
        self.assertEqual((libraries['candidate'], dsos['candidate']),
                         ('/workspace/.zil-environment/llvm-function-products-v18/lib',
                          '785a2413d05149ba84789a588f5ca2d4d6b1ef34248ca282789acd4f3133c1d2'))
        arguments = self.arguments()
        arguments.update(candidate_source=source, candidate_llvm_library_path=libraries['candidate'])
        validator.check_arguments(arguments, source)
        record = {'source': source, 'binary_sha256': validator.CANDIDATES[source],
                  'command': validator.expected_controller_command(Path('/tools'), Path('/raw'), arguments)}
        validator.check_controller_record(record, Path('/tools'), Path('/raw'), arguments, source)
        validator.check_build_runtime({'path': libraries['candidate']+'/libLLVM.so.23.1',
                                       'sha256': dsos['candidate']}, libraries, dsos)
        for field, wrong in [('modules', 19), ('functions', 19), ('steps', 15),
                             ('case', 'mixed'), ('mode', 'O0'), ('backend', 'clang'),
                             ('blocks', 2), ('request', ['cold', 'private']),
                             ('cache', 'none'), ('capacity', 255), ('codegen_units', 1),
                             ('codegen_jobs', 2), ('source_cache', True), ('closed_executable', True),
                             ('candidate_extra_arg', ['--push-reuse']),
                             ('candidate_extra_arg', ['--push-reuse', '--profile-compilation', '--skip-verify']),
                             ('extra_arg', ['--max-nodes', '0']),
                             ('baseline_extra_arg', ['--profile-compilation']),
                             ('candidate_llvm_library_path', validator.SDK15_RUNTIME[0]),
                             ('candidate_llvm_library_path', '/workspace/.zil-environment/llvm-function-products-v16/lib')]:
            bad = copy.deepcopy(arguments)
            bad[field] = wrong
            with self.subTest(field=field, wrong=wrong), self.assertRaises(ValueError):
                validator.check_arguments(bad, source)
        for old_source in validator.CANDIDATES:
            if old_source != source:
                with self.subTest(old_source=old_source), self.assertRaises(ValueError):
                    validator.check_controller_record(dict(record, binary_sha256=validator.CANDIDATES[old_source]),
                                                     Path('/tools'), Path('/raw'), arguments, source)
        for unknown in ('7bac1cc5b', 'HEAD', 'f' * 40):
            with self.subTest(unknown=unknown), self.assertRaises(ValueError):
                validator.runtime_pins(unknown)

    def test_runtime_environment_is_source_pinned_without_optional_config_wildcard(self):
        config = '/workspace/.zil-environment/llvm-23.1.4/bin/llvm-config'
        for source in validator.CANDIDATES:
            for arm in ('baseline', 'candidate'):
                expected = validator.runtime_environment_pin(arm, source)
                self.assertEqual(expected['ZIL_LLVM_CONFIG'],
                                 None if source == validator.ELF_ARCHIVE_SOURCE else config)
                validator.check_runtime_environment(expected, arm, source)
                for field, value in [('ZIL_LLVM_CONFIG', config if source == validator.ELF_ARCHIVE_SOURCE else None),
                                     ('LLVM_CONFIG', config), ('LD_PRELOAD', '/tmp/inject.so'),
                                     ('LD_AUDIT', '/tmp/audit.so'), ('ZIL_CLANG', '/other/clang'),
                                     ('ZIL_LLVM_LIB_DIR', '/other/lib'), ('LD_LIBRARY_PATH', '/other/lib')]:
                    with self.subTest(source=source, arm=arm, field=field), self.assertRaises(ValueError):
                        validator.check_runtime_environment(dict(expected, **{field: value}), arm, source)
                missing = dict(expected)
                del missing['ZIL_LLVM_CONFIG']
                with self.assertRaises(ValueError):
                    validator.check_runtime_environment(missing, arm, source)


if __name__ == '__main__':
    unittest.main()
