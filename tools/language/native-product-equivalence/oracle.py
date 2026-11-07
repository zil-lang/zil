#!/usr/bin/env python3
"""Independent source operations and frozen inputs, never machine observations."""
import hashlib
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / 'tests/comptime-generated-items'
RULE = 'ZIL-LANG-1.4'
GRAMMAR_PREFIX = b'zil/provider-grammar/v1\0'


class AllocationFailure(Exception):
    pass


class Source:
    def __init__(self, fail_at=0):
        self.output = []
        self.fail_at = fail_at
        self.calls = self.allocated = self.frees = self.failures = self.live = self.peak = 0

    def emit(self, code):
        self.output.append(chr(code))

    def reserve(self):
        self.calls += 1
        if self.calls == self.fail_at:
            self.failures += 1
            raise AllocationFailure()
        self.live += 1
        self.allocated += 1
        self.peak = max(self.peak, self.live)

    def release(self):
        assert self.live == 1
        self.live -= 1
        self.frees += 1

    def run(self, case):
        result = 0
        if case == 'frozen_type_value':
            value = 1
            result = value - 1
        elif case == 'type_values':
            values = [1, 2, 3, 4]
            point = {'x': 5, 'y': 8}
            # x86_64 target: two i32 fields, each size/alignment 4.
            size = 4 + 4
            alignment = 4
            assert values[3] == 4 and point['x'] == 5 and size == 8 and alignment == 4
        elif case == 'four_categories':
            caller_tmp = 90
            for _ in range(2):
                fragment_tmp = 75
                self.emit(fragment_tmp)
            self.emit(caller_tmp)
            value = 41 + 1
            answer = 42
            self.emit(48 + value // 10)
            self.emit(48 + value % 10)
            result = answer - 42
        elif case == 'providers_hygiene':
            count = 0
            caller_tmp, support_local = 90, 999
            count += 1  # evaluate next(&mut count) exactly once
            generated_tmp = 65
            self.emit(generated_tmp)
            self.emit(generated_tmp)
            self.emit(caller_tmp)
            value, scaled, from_provider = 41 + 1, 21 * 2, 7
            assert count == 1 and caller_tmp == 90 and support_local == 999
            assert value == scaled == 42 and from_provider == 7
            self.emit(48 + value // 10)
            self.emit(48 + value % 10)
        elif case == 'generated_ownership':
            for early in [False, True]:
                active = [65]
                if not early:
                    active.append(66)
                for ticket in reversed(active):
                    self.emit(ticket)
            # A declaration of never_called creates no runtime Ticket.
            self.emit(80)  # main's GeneratedOwner owns P until its return
        elif case in ['generated_allocation', 'generated_early_error']:
            allocated = False
            try:
                if case.startswith('05b_'):
                    raise AllocationFailure()  # source error, no allocator request
                self.reserve()
                allocated = True
                # push(88) into capacity 1 does not make an allocation request.
                assert [88][0] == 88
            except AllocationFailure:
                result = 1
            finally:
                self.emit(70)  # same Ticket F, either local or returned package
                if allocated:
                    self.release()
        elif case == 'generated_impl':
            self.emit(84)
        elif case == 'fresh_and_named':
            # A local CT value named i32 is the argument Type i64, not the
            # built-in type with that spelling. Runtime Selected is i64.
            selected = 'i64'
            assert selected == 'i64'
            caller_tmp = 90
            fresh_slots = [object(), object()]
            assert fresh_slots[0] is not fresh_slots[1]
            for _ in fresh_slots:
                self.emit(75)
            self.emit(caller_tmp)
            result = (41 + 1) - 42
        else:
            raise AssertionError(case)
        assert not self.live
        return dict(exit=result, stdout=''.join(self.output), alloc_calls=self.calls,
                    allocated=self.allocated, frees=self.frees, failures=self.failures,
                    live=self.live, peak_live=self.peak)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def modules(entry, handwritten=False):
    """Ordinary logical import graph; reserved CT libraries are compiler inputs."""
    base = FIXTURES / ('handwritten' if handwritten else 'project')
    found = {}
    pending = [(None, base / entry)]
    while pending:
        logical, path = pending.pop()
        key = logical or '<root>'
        if key in found:
            continue
        found[key] = path
        for name in re.findall(r'^(?:import|syntax use) ([A-Za-z0-9_.]+)(?: as \w+)?', path.read_text(), re.M):
            if name in ['std.meta', 'std.syntax']:
                continue
            dependency = base.joinpath(*name.split('.')).with_suffix('.zil')
            if not dependency.is_file():
                dependency = (FIXTURES / 'project').joinpath(*name.split('.')).with_suffix('.zil')
            assert dependency.is_file(), (entry, name)
            pending.append((name, dependency))
    ordered = ['<root>', *sorted(key for key in found if key != '<root>')]
    return [dict(logical=key, source=str(found[key].relative_to(FIXTURES)),
                 sha256=digest(found[key])) for key in ordered]


def provider_lock(named=False):
    names = ['expressions'] if named else ['expressions', 'scaling', 'types', 'statements', 'items']
    categories = dict(expressions='Expr', scaling='Expr', types='Type',
                      statements='StmtList', items='ItemList')
    lines = ['zil-syntax-lock 1', 'rule ' + RULE]
    for name in names:
        source = FIXTURES / 'project/providers' / (name + '.zil')
        source_hash = digest(source)
        grammar_hash = hashlib.sha256(GRAMMAR_PREFIX + RULE.encode() + b'\0' + bytes.fromhex(source_hash)).hexdigest()
        for export in (['default', 'named'] if named else ['default']):
            lines.append(f'provider providers.{name} {export} expand {categories[name]} paren,bracket,brace {source_hash} {grammar_hash}')
    return '\n'.join(lines) + '\n'


def build():
    original = json.loads((FIXTURES / 'preparation/SOURCE-MANIFEST.json').read_text())
    for item in original['files']:
        path = FIXTURES / item['path']
        assert path.stat().st_size == item['bytes'] and digest(path) == item['sha256']
    cases = []
    for path in sorted((FIXTURES / 'project').glob('[0-9]*_main.zil')):
        case = path.stem.removesuffix('_main')
        normal = Source().run(case)
        rows = [dict(fail_at=i, **Source(i).run(case)) for i in range(normal['alloc_calls'] + 2)]
        lock = 'providers-five.lock' if case.startswith('03_') else 'provider-named.lock' if case.startswith('07_') else None
        cases.append(dict(name=case, entry=path.name, input_hex='', normal=normal,
                          allocator=rows if case.startswith(('05_', '05b_')) else [],
                          syntax_lock=lock, generated_modules=modules(path.name),
                          handwritten_modules=modules(path.name, handwritten=True)))
    originals = json.loads((FIXTURES / 'preparation/EXPECTED.json').read_text())
    # Check independent derivation against original future expectations without
    # changing or promoting their unexecuted status.
    by_entry = {case['entry']: case for case in cases}
    for item in originals['positive']:
        actual = by_entry[item['entry']]['normal']
        assert (actual['exit'], actual['stdout']) == (item['expected_exit'], item['expected_stdout_ascii'])
    extra = {
        'neg_type_in_runtime_main.zil': ('ct-only-value-in-runtime', 'A Type value cannot cross an explicit runtime foreign ABI, including from unsafe code.'),
        'neg_ident_in_runtime_main.zil': ('ct-only-value-in-runtime', 'A fresh Ident cannot cross an explicit runtime foreign ABI, including from unsafe code.'),
        'neg_export_cannot_capture_main.zil': ('hygiene-no-callsite-authority', 'Export intent cannot capture the caller private_local as an input binding.'),
        'neg_type_splice_expr_main.zil': ('syntax-category-mismatch', 'A type splice requires Syntax<Type>, never Syntax<Expr>.'),
        'neg_item_emit_statements_main.zil': ('syntax-category-mismatch', 'An item-list receiver cannot emit Syntax<StmtList>.'),
        'neg_late_import_main.zil': ('sealed-discovery-graph', 'A late generated import cannot modify the sealed project discovery graph.'),
    }
    negatives = []
    for path in sorted((FIXTURES / 'project').glob('neg_*_main.zil')):
        if path.name in extra:
            key, obligation = extra[path.name]
        else:
            item = next(item for item in originals['negative'] if item['entry'] == path.name)
            key, obligation = item['semantic_error_key_proposal'], item['obligation']
        negatives.append(dict(entry=path.name, required_result='SafeReject', semantic_key=key,
                              obligation=obligation, native_runs=0, unrelated_Unsupported_is_not_Pass=True,
                              modules=modules(path.name), syntax_lock='provider-named.lock' if path.name == 'neg_unknown_provider_export_main.zil' else None))
    return dict(evidence_level='independent source-operation oracle; not compiler/native evidence',
                contract='_zil_design.md §16/16.7/18.2 at 1b8f0963d293b4d7354c2a865c13da7c170a0157',
                target='x86_64 LLVM host, i32 size/alignment4; source i64 metadata conversion',
                original_source_manifest_sha256=digest(FIXTURES / 'preparation/SOURCE-MANIFEST.json'),
                provider_lock_contract='261 strict text version1; explicit CLI and identical in-memory bytes',
                positive=cases, negative=negatives,
                source_allocation_counts='predicted from source operations and existing ByteBuffer runtime; never claimed observed',
                observations=dict(parsed=0, compiled=0, native=0))


if __name__ == '__main__':
    locks = FIXTURES / 'locks'
    if sys.argv[1:] == ['--check']:
        assert (locks / 'providers-five.lock').read_text() == provider_lock()
        assert (locks / 'provider-named.lock').read_text() == provider_lock(named=True)
        assert json.loads((FIXTURES / 'ORACLE.json').read_text()) == build()
        print('frozen source, independent oracle and explicit locks unchanged')
    else:
        assert not sys.argv[1:]
        locks.mkdir(exist_ok=True)
        (locks / 'providers-five.lock').write_text(provider_lock())
        (locks / 'provider-named.lock').write_text(provider_lock(named=True))
        (FIXTURES / 'ORACLE.json').write_text(json.dumps(build(), indent=2) + '\n')
