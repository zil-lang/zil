#!/usr/bin/env python3
"""Separate private-slot source oracle. Never reads a compiler or machine output."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[3]
SOURCES = ROOT / 'tests/callable-instance-forwarding'
spec = importlib.util.spec_from_file_location(
    'impl256_supplement_source_operations', SOURCES / 'frozen-prep/oracle.py')
legacy = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = legacy
spec.loader.exec_module(legacy)


class Source(legacy.Source):
    def ordinary(self, call):
        # Mark M precedes the environment buffer. Until reserve succeeds the
        # mark is a local, then the closure owns it and the buffer together.
        try:
            allocation = self.allocate()
        except legacy.Failure:
            self.emit('M')
            raise
        callback = legacy.Callback('ordinary-input-M', 'FnMut', 'FnMut', 'M', allocation)
        try:
            if call:
                for mark, code in [('A', 41), ('B', 40)]:
                    # The mutable callee is prepared without consuming its
                    # environment. A failed explicit argument skips the body.
                    event = self.event(mark)
                    callback.count += 1
                    self.emit(str(callback.count))
                    assert code + callback.count == 42
                    self.emit('R')
                    self.drop_event(event)
                assert callback.count == 2
        finally:
            self.drop_callback(callback)
        assert not self.forwarded, 'this entire source graph has no forwarding call'

    def mixed(self, right):
        # Both have the marker factory's same concrete ClosureId. Distinct
        # input identities and owning markers remain distinct across the join.
        first = legacy.Callback('mixed-input-A', 'Fn', 'Fn', 'A')
        second = legacy.Callback('mixed-input-B', 'Fn', 'Fn', 'B')
        selected, unselected = (second, first) if right else (first, second)
        current = self.forward(selected)
        assert current is selected and current is not unselected
        self.marker_call(current)
        self.drop_callback(current)
        self.drop_callback(unselected)

    def run_supplement(self, group, mode):
        self.emit('S')
        exit_code = 0
        try:
            if group == 'ordinary':
                self.ordinary(mode == 0)
            else:
                assert group == 'mixed'
                self.mixed(mode == 1)
        except legacy.Failure:
            exit_code = 1
        finally:
            self.emit('Q')
        assert not self.live
        return dict(exit=exit_code, stdout=''.join(self.output), alloc_calls=self.calls,
                    allocated=self.allocated, frees=self.frees, failures=self.failures,
                    live=0, peak_live=self.peak, forwarded_instance_trace=self.forwarded)


def build():
    groups = {}
    for group in ['ordinary', 'mixed']:
        groups[group] = []
        for mode in range(2):
            normal = Source().run_supplement(group, mode)
            groups[group].append(dict(input=str(mode), normal=normal,
                allocator=[dict(fail_at=i, **Source(i).run_supplement(group, mode))
                           for i in range(normal['alloc_calls'] + 2)]))
    assert [c['normal']['stdout'] for c in groups['ordinary']] == ['S1RA2RBMQ', 'SMQ']
    assert [c['stdout'] for c in groups['ordinary'][0]['allocator']] == [
        'S1RA2RBMQ', 'SMQ', 'SAMQ', 'S1RABMQ', 'S1RA2RBMQ']
    assert [c['normal']['stdout'] for c in groups['mixed']] == ['SaABQ', 'SbBAQ']
    paths = ['supplement/ordinary_main.zil', 'supplement/mixed_main.zil',
             'frozen-prep/project/model.zil', 'frozen-prep/project/relay.zil']
    return dict(evidence_level='independent source-operation expectations; not native evidence',
                contract='_zil_design.md §9.3.1 and §9.4.2; structural private carrier proposal bfbb677ec',
                source_sha256={p: hashlib.sha256((SOURCES / p).read_bytes()).hexdigest()
                               for p in paths},
                frozen_core_oracle_sha256=hashlib.sha256((SOURCES / 'ORACLE.json').read_bytes()).hexdigest(),
                planned_separate_group=dict(normal_modes=4, native_O0_O2_runs=32,
                    normal_runs=8, allocator_runs=24, real_allocator_failures=8),
                **groups)


if __name__ == '__main__':
    (SOURCES / 'supplement/ORACLE.json').write_text(json.dumps(build(), indent=2) + '\n')
