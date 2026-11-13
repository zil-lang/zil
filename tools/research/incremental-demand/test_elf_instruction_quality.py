#!/usr/bin/env python3
"""Pure byte fixtures; no compiler, linker, generated program, or tool execution."""
import copy
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

MODULE = Path(__file__).with_name('elf_instruction_quality.py')
SPEC = importlib.util.spec_from_file_location('elf_instruction_quality', MODULE)
q = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(q)


def fixture(functions, base=0x1000, data=None, extras=()):
    """(name, [(encoding, assembly)]) -> actual ELF64 bytes + textual tool fixtures."""
    text, assembly, syms = bytearray(), [], []
    for name, instructions in functions:
        start = base + len(text)
        assembly.append(f'{start:016x} <{name}>:')
        for encoded, asm in instructions:
            assembly.append(f' {base+len(text):x}:\t{encoded.hex(" ")}\t{asm}')
            text.extend(encoded)
        syms.append((name, start, len(text) - (start-base), 'FUNC', 1))
    sections = [('', 0, 0, 0, b''), ('.text', 1, 6, base, bytes(text))]
    if data is not None:
        sections.append(('.data', 1, 3, 0x3000, data))
        syms.append(('value', 0x3000, len(data), 'OBJECT', 2))
    sections.extend(extras)
    name_bytes = b'\0' + b''.join(name.encode()+b'\0' for name in
                                  [row[0] for row in sections[1:]] + ['.shstrtab'])
    sections.append(('.shstrtab', 3, 0, 0, name_bytes))
    payload, headers = bytearray(64), []
    for name, kind, flags, address, contents in sections:
        offset = len(payload)
        payload.extend(contents)
        headers.append((name_bytes.index(name.encode()+b'\0') if name else 0,
                        kind, flags, address, offset, len(contents), 0, 0, 1, 0))
    shoff = len(payload)
    for header in headers:
        payload.extend(struct.pack('<IIQQQQIIQQ', *header))
    ident = b'\x7fELF\x02\x01\x01' + bytes(9)
    struct.pack_into('<16sHHIQQQIHHHHHH', payload, 0, ident, 2, 62, 1, base,
                     0, shoff, 0, 64, 0, 0, 64, len(headers), len(headers)-1)
    symtext = '\n'.join(f' {i}: {address:016x} {size} {kind} GLOBAL DEFAULT {section} {name}'
                        for i, (name, address, size, kind, section) in enumerate(syms, 1))
    return bytes(payload), symtext, '\n'.join(assembly), ''


def receipt(removed=('gone',)):
    return {'policy': q.POLICY, 'candidates': list(removed), 'eligible': list(removed),
            'before_definitions': ['live', *removed], 'after_definitions': ['live'],
            'removed': list(removed), 'ran_globaldce': True, 'cost_ns': 1}


def unit(value=None, key='a'*64, sha='b'*64):
    result = {'cache': {'action_key': key, 'state': 'miss' if value is not None else 'hit'},
              'object_fingerprint': sha}
    if value is None:
        result['late_closed_policy'] = {'policy': q.POLICY, 'execution': 'object-cache',
                                       'producer_action_key': key, 'candidates': ['gone']}
    else:
        result['late_closed'] = value
    return result


def row_with_units(*units):
    return {'resident': {'report': {'native_codegen': {'actual_units': len(units), 'object_reports': list(units)}}}}


class Instructions(unittest.TestCase):
    def image(self, value):
        return q.Image(*value)

    def test_moved_direct_call_and_cfg_are_equal(self):
        def pair(base):
            return fixture([('caller', [(b'\xe8\x01\0\0\0', f'call {base+6:x} <callee>'),
                                         (b'\xc3', 'ret')]), ('callee', [(b'\xc3', 'ret')])], base)
        self.assertEqual(self.image(pair(0x1000)).normalized(), self.image(pair(0x2000)).normalized())
        changed = fixture([('caller', [(b'\xe8\xfb\xff\xff\xff', 'call 1000 <caller>'),
                                       (b'\xc3', 'ret')]), ('callee', [(b'\xc3', 'ret')])])
        self.assertNotEqual(self.image(pair(0x1000)).normalized(), self.image(changed).normalized())

    def test_immediate_opcode_register_size_and_cfg_changes_remain(self):
        def build(code, text):
            return self.image(fixture([('live', [(code, text), (b'\xc3', 'ret')])])).normalized()
        a = build(b'\xb8\x01\0\0\0', 'mov eax,0x1')
        for code, text in [(b'\xb8\x02\0\0\0', 'mov eax,0x2'),
                           (b'\xb9\x01\0\0\0', 'mov ecx,0x1'),
                           (b'\x31\xc0', 'xor eax,eax')]:
            self.assertNotEqual(a, build(code, text))
        a = self.image(fixture([('live', [(b'\xeb\x00', 'jmp 1002 <live+0x2>'), (b'\xc3', 'ret')])])).normalized()
        b = self.image(fixture([('live', [(b'\x74\x00', 'je 1002 <live+0x2>'), (b'\xc3', 'ret')])])).normalized()
        self.assertNotEqual(a, b)

    def test_rip_data_bytes_are_bound(self):
        def build(data):
            code = b'\x48\x8b\x05' + (0x3000-0x1007).to_bytes(4, 'little')
            return self.image(fixture([('live', [(code, 'mov rax,QWORD PTR [rip+0x1ff9] # 3000 <value>'),
                                                (b'\xc3', 'ret')])], data=data)).normalized()
        self.assertNotEqual(build(b'12345678'), build(b'12345679'))

    def test_external_plt_call_binds_actual_stub_and_symbol_version(self):
        f = list(fixture([('live', [(b'\xe8\xfb\x0f\0\0', 'call 2000 <puts@plt>'),
                                    (b'\xc3', 'ret')])], data=bytes(8),
                         extras=[('.plt', 1, 6, 0x2000, b'\xff\x25\xfa\x0f\0\0')]))
        f[2] += '\n0000000000002000 <puts@plt>:\n 2000:\tff 25 fa 0f 00 00\tjmp QWORD PTR [rip+0xffa] # 3000 <value>'
        f[3] = '0000000000003000 0000000000000001 R_X86_64_JUMP_SLOT 0000000000000000 puts@GLIBC_2.2.5 + 0'
        before = self.image(f).normalized()
        f[3] = f[3].replace('puts@GLIBC_2.2.5', 'puts@GLIBC_2.3')
        self.assertNotEqual(before, self.image(f).normalized())
        f[2] = f[2].replace('ff 25 fa', 'ff 25 fb')
        with self.assertRaises(q.QualityError): self.image(f)

    def test_unknown_bytes_references_and_incomplete_extents_refuse(self):
        f = fixture([('live', [(b'\xc3', 'ret')])])
        bad = list(f); bad[2] = bad[2].replace('c3', '90')
        with self.assertRaises(q.QualityError): self.image(bad)
        bad = list(f); bad[1] = bad[1].replace(' 1 FUNC', ' 2 FUNC')
        with self.assertRaises(q.QualityError): self.image(bad)
        bad = list(f); bad[1] = bad[1].replace(' FUNC ', ' IFUNC ')
        with self.assertRaises(q.QualityError): self.image(bad)
        with self.assertRaises(q.QualityError):
            self.image(fixture([('live', [(b'\xe8\xfb\x0f\0\0', 'call 2000 <unknown>')])])).normalized()
        with self.assertRaises(q.QualityError): q.disassembly(' 1000:\t00\t(bad)')
        with self.assertRaises(q.QualityError):
            q.relocation_rows('0000000000003000 0000000000000001 R_X86_64_COPY 0000 value + 0')

    def test_zero_size_function_extent_remains_complete(self):
        value = list(fixture([('live', [(b'\x90', 'nop'), (b'\xc3', 'ret')])]))
        value[1] = value[1].replace(' 2 FUNC', ' 0 FUNC')
        result = self.image(value).normalized()['live']
        self.assertTrue(result['derived_zero_size'])
        self.assertEqual(len(result['instructions']), 2)


class Receipts(unittest.TestCase):
    def test_exact_sets_and_policy_are_required(self):
        q.checked_receipt(receipt())
        for key, value in [('removed', ['other']), ('after_definitions', ['new']),
                           ('eligible', ['other']), ('ran_globaldce', False), ('policy', 'other')]:
            bad = receipt(); bad[key] = value
            with self.assertRaises(q.QualityError): q.checked_receipt(bad)

    def test_hit_requires_same_actual_bytes_policy_and_validated_producer(self):
        removed, pending = q.deletion_evidence(row_with_units(unit(receipt())), {}, True)
        self.assertEqual(removed, {'gone'})
        self.assertEqual(q.deletion_evidence(row_with_units(unit()), pending, False)[0], {'gone'})
        for candidate, cold, index in [(unit(), True, pending), (unit(sha='c'*64), False, pending),
                                       (unit(key='d'*64), False, pending), (unit(), False, {})]:
            with self.assertRaises(q.QualityError): q.deletion_evidence(row_with_units(candidate), index, cold)
        bad = unit(); bad['late_closed_policy']['policy'] = 'wrong'
        with self.assertRaises(q.QualityError): q.deletion_evidence(row_with_units(bad), pending, False)
        fresh = receipt(); fresh['cost_ns'] = 999
        q.deletion_evidence(row_with_units(unit(fresh)), pending, False)

    def test_inventory_duplicate_partial_or_hash_mismatch_refuses(self):
        row = row_with_units(unit(receipt()))
        item = {'unit_index': 0, 'action_key': 'a'*64, 'object': {'sha256': 'b'*64}}
        row['inventory'] = {'objects': [item]}
        q.deletion_evidence(row, {}, True)
        for items in [[item, item], [dict(item, unit_index=1)],
                      [dict(item, object={'sha256': 'c'*64})]]:
            row['inventory']['objects'] = items
            with self.assertRaises(q.QualityError): q.deletion_evidence(row, {}, True)


class PairIntegration(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.out = Path(self.tmp.name)
        self.artifacts = {}
        self.base = fixture([('live', [(b'\xc3', 'ret')]), ('gone', [(b'\xc3', 'ret')])])
        self.candidate = fixture([('live', [(b'\xc3', 'ret')])])
        fake_tools = {}
        for name in q.TOOLS:
            path = self.out / name; path.write_bytes(name.encode()); fake_tools[name] = path
        self.patches = [patch.object(q, 'TOOLS', fake_tools), patch.object(q, 'tool', self.fake_tool)]
        for p in self.patches: p.start()
        q.dump(self.out/'manifest.json', {'source_shas': {'candidate': 'a'*40, 'baseline': 'c'*40},
               'binaries': {'candidate': {'sha256': 'b'*64}, 'baseline': {'sha256': 'f'*64}},
               'native_quality_protocol': {'files': [{'path': MODULE.name,
                                                    'sha256': q.digest(MODULE.read_bytes())}]}})
        for arm, value in [('baseline', self.base), ('candidate', self.candidate)]:
            (self.out/arm).write_bytes(value[0]); self.artifacts[q.digest(value[0])] = value

    def tearDown(self):
        for p in reversed(self.patches): p.stop()
        self.tmp.cleanup()

    def fake_tool(self, name, flags, elf, directory):
        value = self.artifacts[q.digest(elf.read_bytes())]
        text = value[1] if flags == ['-Ws'] else value[3] if flags == ['-rW'] else value[2] if name == 'objdump' else ''
        stem = name + '-' + '-'.join(x.strip('-') for x in flags)
        for suffix, content in [('stdout', text), ('stderr', ''), ('version', 'fake fixture')]:
            (directory/(stem+'.'+suffix)).write_text(content)
        q.dump(directory/(stem+'.command.json'), {'fixture': True})
        return text

    def pair(self, request='cold', index=0, block=0, fresh=True):
        result = {}
        for arm in ['baseline', 'candidate']:
            path = self.out/arm
            report = {'status': 'Success', 'verification_runs': 1, 'diagnostics': [],
                      'artifacts': {'executable': str(path)},
                      'native_codegen': {'actual_units': 1, 'object_reports': [unit(receipt() if fresh else None)]}}
            result[arm] = {'block': block, 'request_index': index, 'request': request, 'measured': True,
                           'valid': True, 'resident': {'report': report}, 'native': [{'valid': True}],
                           'executable_sha256': q.digest(path.read_bytes()),
                           'llvm_comparison_sha256_excluding_identity_comment': 'd'*64,
                           'source_identity_sha256': '9'*64,
                           'runtime': {'llvm_dsos': [{'sha256': 'e'*64}]}}
        return result

    def test_fresh_pair_then_sameblock_hit_and_archived_raw(self):
        result = q.compare_pair(self.pair(), self.out)
        self.assertTrue(result['valid']); self.assertEqual(result['deleted_functions'], ['gone'])
        self.assertEqual(len(list((self.out/'closed-machine-quality'/'elf').glob('*/addressed.elf'))), 2)
        result = q.compare_pair(self.pair('noop', 1, fresh=False), self.out)
        self.assertTrue(result['valid'])
        self.assertTrue((self.out/'closed-machine-quality'/'producer-index-block-0.json').exists())
        with self.assertRaises(q.QualityError): q.compare_pair(self.pair('noop', 2, block=1, fresh=False), self.out)
        with self.assertRaises(q.QualityError): q.compare_pair(self.pair('cold', 0, block=2, fresh=False), self.out)

    def test_raw_elf_identity_skips_decoder_but_never_producer_or_current_checks(self):
        (self.out/'candidate').write_bytes((self.out/'baseline').read_bytes())
        with patch.object(q, 'tool', side_effect=AssertionError('raw identity must not run analysis tools')), \
             patch.object(q, 'Image', side_effect=AssertionError('raw identity must not decode')):
            result = q.compare_pair(self.pair(), self.out)
            self.assertTrue(result['valid'])
            self.assertEqual(result['proof'], 'raw_elf_identity')
            self.assertFalse(result['machine_normalization_used'])
            self.assertFalse(result['machine_normalization_executed'])
            index_path = self.out/'closed-machine-quality'/'producer-index-block-0.json'
            self.assertEqual(len(json.loads(index_path.read_text())['units']), 1)
            parent = self.pair('cold', 1, fresh=False)
            for row in parent.values(): row['measured'] = False
            self.assertTrue(q.compare_pair(parent, self.out)['valid'])
            for number, mode in enumerate(['missing_receipt', 'cold_hit', 'cross_block_hit',
                                            'unknown_object', 'verify', 'native', 'source', 'llvm',
                                            'failed_status'], 2):
                with self.subTest(mode=mode):
                    pair = self.pair('noop', number, fresh=False)
                    candidate = pair['candidate']
                    report = candidate['resident']['report']
                    if mode in {'missing_receipt', 'cold_hit'}:
                        for row in pair.values(): row.update(request='cold', block=number)
                        if mode == 'missing_receipt':
                            report['native_codegen']['object_reports'][0]['cache']['state'] = 'miss'
                    elif mode == 'cross_block_hit':
                        for row in pair.values(): row['block'] = number
                    elif mode == 'unknown_object':
                        report['native_codegen']['object_reports'][0]['object_fingerprint'] = '0'*64
                    elif mode == 'verify': report['verification_runs'] = 0
                    elif mode == 'native': candidate['native'][0]['valid'] = False
                    elif mode == 'source': candidate['source_identity_sha256'] = '0'*64
                    elif mode == 'llvm': candidate['llvm_comparison_sha256_excluding_identity_comment'] = '0'*64
                    elif mode == 'failed_status': report['status'] = 'SafeReject'
                    with self.assertRaises(q.QualityError): q.compare_pair(pair, self.out)
                    if mode in {'missing_receipt', 'cold_hit', 'cross_block_hit'}:
                        self.assertFalse(index_path.with_name(f'producer-index-block-{number}.json').exists())
            self.assertFalse(list((self.out/'closed-machine-quality'/'elf').glob('*/analysis.json')))

    def test_failure_preserves_elf_and_does_not_publish_producer(self):
        pair = self.pair(); pair['candidate']['resident']['report']['native_codegen']['object_reports'][0]['late_closed'] = receipt(())
        with self.assertRaises(q.QualityError): q.compare_pair(pair, self.out)
        root = self.out/'closed-machine-quality'
        self.assertFalse((root/'producer-index-block-0.json').exists())
        self.assertEqual(len(list(root.glob('elf/*/addressed.elf'))), 2)
        result = json.loads(next(root.glob('block-*/result.json')).read_text())
        self.assertFalse(result['valid']); self.assertIn('absent', result['error'])

    def test_raw_transcript_cache_corruption_refuses(self):
        q.compare_pair(self.pair(), self.out)
        path = next((self.out/'closed-machine-quality'/'elf').glob('*/objdump*.stdout'))
        path.write_text('corrupt')
        with self.assertRaises(q.QualityError): q.compare_pair(self.pair('noop', 1, fresh=False), self.out)

    def test_changed_retained_code_or_current_verify_rejects(self):
        pair = self.pair(); pair['candidate']['resident']['report']['verification_runs'] = 0
        with self.assertRaises(q.QualityError): q.compare_pair(pair, self.out)
        self.assertFalse((self.out/'closed-machine-quality'/'producer-index-block-0.json').exists())
        value = fixture([('live', [(b'\x90', 'nop'), (b'\xc3', 'ret')])])
        (self.out/'candidate').write_bytes(value[0]); self.artifacts[q.digest(value[0])] = value
        pair = self.pair(index=1)
        with self.assertRaises(q.QualityError): q.compare_pair(pair, self.out)


if __name__ == '__main__':
    unittest.main()
