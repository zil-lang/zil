#!/usr/bin/env python3
"""Small adversarial checks for the diagnostic-only profile qualifier."""
import copy
import importlib.util
from pathlib import Path
import unittest

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


if __name__ == '__main__':
    unittest.main()
