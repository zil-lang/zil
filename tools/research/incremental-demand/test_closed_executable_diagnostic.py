#!/usr/bin/env python3
"""Tiny oracle checks only: no compiler, ELF execution or benchmark."""
import importlib.util
from pathlib import Path
import sys
import unittest

import push_counters as pc

NATIVE = Path(__file__).resolve().parents[1] / 'reactive-query-graph'
sys.path.insert(0, str(NATIVE))
spec = importlib.util.spec_from_file_location('fixed_native_measure', NATIVE / 'native_measure.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


class LLVMIdentityTests(unittest.TestCase):
    def test_runner_uses_the_same_fixed_matched_helper(self):
        self.assertEqual(native.COMPARISON_HELPER.resolve(), Path(pc.__file__).resolve())
        body = b'target triple = "x86_64-unknown-linux-gnu"\ndefine i32 @main() { ret i32 0 }\n'
        def module(identity=b'a' * 64, tail=body):
            return b'; ZIL verified LLVM module\n; verified-fingerprint: ' + identity + b'\n' + tail
        first, second = module(), module(b'b' * 64)
        self.assertEqual(native.llvm_comparison_sha256(first), pc.llvm_comparison_sha256(second))
        for malformed in (b'\n' + first, first.replace(b'\n', b'\r\n'), module(b'g' * 64),
                          module(b'a' * 63), first + b'; verified-fingerprint: ' + b'c' * 64 + b'\n',
                          first.replace(b'; ZIL verified LLVM module\n', b'; ZIL verified LLVM module\n\n')):
            with self.subTest(data=malformed[:100]), self.assertRaises(ValueError):
                native.llvm_comparison_sha256(malformed)
        for tail in (body.replace(b'i32 0', b'i32 1'), body + b'; another comment\n',
                     body.replace(b'x86_64', b'aarch64')):
            self.assertNotEqual(pc.llvm_comparison_sha256(first), native.llvm_comparison_sha256(module(tail=tail)))

    def pair(self, raw=('a', 'b'), compared=('c', 'c'), elf=('d', 'd')):
        return [dict(status='Success', llvm_text_sha256=r,
                     llvm_comparison_sha256_excluding_identity_comment=c,
                     executable_sha256=e) for r, c, e in zip(raw, compared, elf)]

    def test_nonpush_requires_raw_llvm_and_complete_elf_equality(self):
        self.assertFalse(native.codegen_identity(self.pair(), 'O2', False)['paired_codegen_identity_valid'])
        self.assertTrue(native.codegen_identity(self.pair(raw=('a', 'a')), 'O2', False)['paired_codegen_identity_valid'])
        self.assertFalse(native.codegen_identity(self.pair(raw=('a', 'a'), elf=('d', 'e')), 'O2', False)['paired_codegen_identity_valid'])

    def test_push_preserves_raw_failure_and_requires_full_other_bytes(self):
        result = native.codegen_identity(self.pair(), 'O2', True)
        self.assertFalse(result['paired_raw_llvm_equal'])
        self.assertTrue(result['paired_llvm_equal_excluding_identity_comment'])
        self.assertTrue(result['paired_codegen_identity_valid'])
        self.assertFalse(native.codegen_identity(self.pair(compared=('c', 'e')), 'O2', True)['paired_codegen_identity_valid'])
        self.assertFalse(native.codegen_identity(self.pair(elf=('d', 'e')), 'O2', True)['paired_codegen_identity_valid'])


if __name__ == '__main__':
    unittest.main()
