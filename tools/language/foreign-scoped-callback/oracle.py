#!/usr/bin/env python3
"""Independent source-operation oracle. Never reads compiler reports or native output."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3] / 'tests/foreign-scoped-callback'


class Failure(Exception):
    pass


class Trap(Exception):
    pass


class Source:
    def __init__(self, mode, fail_at=0):
        self.mode, self.fail_at = mode, fail_at
        self.output = []
        self.calls = self.allocated = self.frees = self.failures = self.peak = 0
        self.live, self.slots = set(), []
        self.bodies = 0

    def emit(self, value):
        self.output.append(value)

    def drop(self, resource):
        label, allocation = resource
        # Event declares Mark before ByteBuffer; ordinary locals exit in reverse
        # successful initialization order. Capture order is first lexical use.
        self.emit('D' + label + ';')
        if allocation is not None:
            self.live.remove(allocation)
            self.frees += 1

    def prepare(self, label):
        self.calls += 1
        if self.calls == self.fail_at:
            self.failures += 1
            self.emit('D' + label + ';')
            raise Failure()
        self.allocated += 1
        self.live.add(self.calls)
        self.peak = max(self.peak, len(self.live))
        return label, self.calls

    def handler(self, resource):
        self.emit('H;')
        return dict(kind='mut', count=0, resource=resource)

    def invoke(self, f, value):
        label = f.get('resource', ('F', None))[0]
        self.emit('F' + label + ';')
        if f['kind'] == 'mut':
            f['count'] += 1
            if value == 2:
                self.attempt(len(self.slots)-1, 'E', 7, guard=True)
            return value + f['count']
        if f['kind'] == 'trap':
            raise Trap()
        return value + (1 if f['kind'] == 'plus' else 0)

    def attempt(self, index, phase, value, guard=False):
        self.emit(f'{phase}{index}:{value}>')
        result = (1 << 32) if guard else (self.invoke(self.slots[index], value) & 0xffffffff)
        self.emit(f'={result:016x};')
        return result

    def body_start(self):
        self.bodies += 1
        self.emit(f'B{self.bodies};')

    def normal_body(self, resource, result):
        self.body_start()
        self.attempt(len(self.slots)-1, 'P', 2)
        self.attempt(len(self.slots)-1, 'P', 3)
        self.drop(resource)
        return result

    def scope(self, f, body):
        index = len(self.slots)
        self.slots.append(f)
        self.emit(f'R{index};')
        if self.mode != 2:
            self.attempt(index, 'S', -(1 << 31) if self.mode == 6 else 0 if self.mode == 8 else 1)
        if self.mode in (2, 3):
            self.slots.pop()
            self.emit(f'N{index};')
            body(False)
            return False, 0
        result = body(True)
        self.emit(f'C{index};')
        self.attempt(index, 'X', 11, guard=True)
        self.slots.pop()
        self.emit(f'Z{index};')
        return True, result

    def owning(self):
        f = self.handler(self.prepare('A'))
        try:
            body = self.prepare('B')
            self.emit('K;')
            self.scope(f, lambda called: self.normal_body(body, -(1 << 31)) if called else self.drop(body))
            self.invoke(f, 4)
        finally:
            self.drop(f['resource'])

    def pair(self):
        first = self.handler(self.prepare('A'))
        second = None
        try:
            second = self.handler(self.prepare('C'))
            a = self.prepare('B')
            self.emit('K;')
            self.scope(first, lambda called: self.normal_body(a, 17))
            b = self.prepare('D')
            self.emit('K;')
            self.scope(second, lambda called: self.normal_body(b, 19))
            self.invoke(first, 4)
            self.invoke(second, 4)
        finally:
            if second:
                self.drop(second['resource'])
            self.drop(first['resource'])

    def nested_inner(self):
        try:
            inner_resource = self.prepare('C')
        except Failure:
            return -31
        inner = dict(kind='mut', count=0, resource=inner_resource)
        try:
            try:
                body_resource = self.prepare('D')
            except Failure:
                return -32
            def body(called):
                self.body_start()
                self.attempt(1, 'P', 2)
                self.attempt(0, 'O', 9)
                self.attempt(1, 'P', 3)
                self.drop(body_resource)
                return 73
            self.scope(inner, body)
            self.invoke(inner, 7)
            self.attempt(0, 'P', 4)
            return 73
        finally:
            self.drop(inner_resource)

    def nested(self):
        outer = self.handler(self.prepare('A'))
        try:
            resource = self.prepare('B')
            self.emit('K;')
            def body(called):
                self.body_start()
                result = self.nested_inner()
                self.drop(resource)
                return result
            _, result = self.scope(outer, body)
            if result < 0:
                return 1
            self.invoke(outer, 5)
            return 0
        finally:
            self.drop(outer['resource'])

    def repeat(self):
        f = self.handler(self.prepare('A'))
        try:
            for label, result in [('B', 17), ('C', 18)]:
                resource = self.prepare(label)
                self.emit('K;')
                self.scope(f, lambda called: self.normal_body(resource, result))
            self.invoke(f, 4)
        finally:
            self.drop(f['resource'])

    def run(self):
        code = 0
        signal = None
        try:
            if self.mode == 0:
                f = dict(kind='plus')
                def body(called):
                    self.body_start()
                    self.attempt(0, 'P', 2)
                    self.attempt(0, 'P', 3)
                    return -17
                self.scope(f, body)
                self.invoke(f, 4)
            elif self.mode in (1, 2, 3):
                self.owning()
            elif self.mode == 4:
                self.pair()
            elif self.mode == 5:
                code = self.nested()
            elif self.mode == 6:
                def body(called):
                    self.body_start()
                    for value in [-(1 << 31), -1, 0, (1 << 31)-1]:
                        self.attempt(0, 'P', value)
                    return (1 << 31)-1
                self.scope(dict(kind='identity'), body)
            elif self.mode == 7:
                self.repeat()
            elif self.mode == 8:
                self.scope(dict(kind='trap', resource=('T', None)), lambda called: 0)
            else:
                assert self.mode == 9
                f = self.handler(self.prepare('A'))
                self.prepare('B')
                def body(called):
                    self.body_start()
                    raise Trap()
                self.scope(f, body)
        except Failure:
            code = 1
        except Trap:
            # Abort never runs language destructors or atexit. No live-zero
            # guarantee is claimed for a process that did not return.
            code, signal = None, 4
        if signal is None:
            assert not self.live and not self.slots
        row = dict(input_hex=bytes([48+self.mode]).hex(), fail_at=self.fail_at,
                   exit=code, signal=signal, stdout=''.join(self.output))
        if signal is None:
            row.update(alloc_calls=self.calls, allocated=self.allocated, frees=self.frees,
                       failures=self.failures, live=0, peak_live=self.peak)
        return row


def build():
    names = ['pure-fn', 'owned-fnmut', 'null-before-callback', 'null-after-callback',
             'two-instances-one-closure-id', 'nested-distinct-closures', 'signed-bounds',
             'same-fnmut-through-loop', 'callback-trap', 'body-trap']
    cases = []
    for mode, name in enumerate(names):
        normal = Source(mode).run()
        faults = [Source(mode, fail).run() for fail in range(1, normal.get('alloc_calls', 0)+2)] if mode < 8 and normal['alloc_calls'] else []
        cases.append(dict(name=name, normal=normal, allocator=faults))
    return dict(level='independent source-operation model; no compilation/native claim', cases=cases)


if __name__ == '__main__':
    (ROOT / 'ORACLE.json').write_text(json.dumps(build(), indent=2) + '\n')
