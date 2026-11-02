#!/usr/bin/env python3
"""Independent source-operation oracle. Never reads compiler/machine output.

Objects below model the declared owning fields, active variants and successful
preparation stack. Allocator requests represent source reserve operations, not
the compiler's generated instructions. Machine observations are separate files.
"""
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


@dataclass
class Event:
    label: str
    allocation: int


@dataclass
class Items:
    allocation: int
    leaves: list[Event]


class Failed(Exception):
    pass


class Source:
    def __init__(self, fail_at):
        self.fail_at = fail_at
        self.calls = self.allocated = self.frees = self.failures = self.peak = 0
        self.live = set()
        self.stdout = []

    def emit(self, label):
        self.stdout.append(label)

    def request(self):
        self.calls += 1
        if self.calls == self.fail_at:
            self.failures += 1
            raise Failed()
        token = self.calls
        self.live.add(token)
        self.allocated += 1
        self.peak = max(self.peak, len(self.live))
        return token

    def free(self, allocation):
        assert allocation in self.live, "oracle duplicate destruction"
        self.live.remove(allocation)
        self.frees += 1

    def drop(self, value):
        if isinstance(value, Event):
            # Event/Reply/Leaf declaration order: Mark, then ByteBuffer.
            self.emit(value.label)
            self.free(value.allocation)
        elif isinstance(value, Items):
            for leaf in value.leaves:
                self.drop(leaf)
            self.free(value.allocation)
        else:
            assert isinstance(value, str)
            self.emit(value)

    def prepare(self, label="A", cancel=False):
        try:
            allocation = self.request()
        except Failed:
            self.drop(label)
            raise
        event = Event(label, allocation)
        if cancel:
            self.drop(event)
            raise Failed()
        return event

    def render(self, reply):
        self.emit("R")
        self.drop(reply)

    def once(self, cancel):
        # Callee is prepared before its event. Failed preparation never enters C.
        try:
            event = self.prepare(cancel=cancel)
        except Failed:
            self.drop("H")
            self.drop("h")
            raise
        self.emit("C")
        reply = event  # all result fields prepared before local/env cleanup
        self.drop("H")
        self.drop("h")
        self.render(reply)

    def later(self, cancel):
        staged = []
        try:
            staged.append(self.prepare("A"))
            staged.append(self.prepare("B", cancel))
        except Failed:
            for value in reversed(staged):
                self.drop(value)
            self.drop("H")
            self.drop("h")
            raise
        self.emit("C")
        reply, second = staged
        # held is a later local than the second formal parameter.
        self.drop("H")
        self.drop(second)
        self.drop("h")
        self.render(reply)

    def repeated(self, mutate):
        for count, label in enumerate("AB", 1):
            event = self.prepare(label)
            self.emit(str(count) if mutate else "C")
            self.render(event)

    def body_error(self):
        try:
            event = self.prepare()
        except Failed:
            self.drop("H")
            self.drop("h")
            raise
        self.emit("C")
        self.drop("H")
        self.drop(event)
        self.drop("h")
        raise Failed()

    def sum(self, empty=False):
        event = None if empty else self.prepare()
        self.emit("C")
        if empty:
            self.emit("E")
        else:
            self.render(event)

    def store(self):
        values = Items(self.request(), [])
        try:
            for label in "AB":
                values.leaves.append(self.prepare(label))
        except Failed:
            self.drop(values)
            raise
        self.emit("C")
        self.render(values)

    def run(self, mode):
        self.emit("S")
        exit_code = 0
        try:
            if mode in (0, 1):
                self.once(mode == 1)
            elif mode in (2, 9):
                self.later(mode == 2)
            elif mode in (3, 4):
                self.repeated(mode == 4)
            elif mode in (5, 6, 8):
                self.sum(mode == 8)
            elif mode == 7:
                self.store()
            elif mode == 10:
                self.body_error()
            else:
                raise AssertionError(mode)
        except Failed:
            exit_code = 1
        finally:
            self.emit("Q")
        assert not self.live, "oracle leaked an owner"
        return dict(exit=exit_code, stdout="".join(self.stdout),
                    alloc_calls=self.calls, allocated=self.allocated,
                    frees=self.frees, failures=self.failures,
                    live=len(self.live), peak_live=self.peak)

    def owned_error(self):
        self.emit("S")
        try:
            event = self.prepare()
        except Failed:
            self.drop("H")
            self.drop("h")
            self.emit("Q")
        else:
            self.emit("C")
            error_payload = event
            self.drop("H")
            self.drop("h")
            self.emit("Q")
            # The entry wrapper owns the error only after caller exit cleanup.
            self.drop(error_payload)
        assert not self.live
        return dict(exit=1, stdout="".join(self.stdout), alloc_calls=self.calls,
                    allocated=self.allocated, frees=self.frees,
                    failures=self.failures, live=0, peak_live=self.peak)


def build():
    cases = []
    for mode in range(11):
        normal = Source(0).run(mode)
        cases.append(dict(input=chr(48+mode), normal=normal,
                          allocator=[dict(fail_at=i, **Source(i).run(mode))
                                     for i in range(normal["alloc_calls"] + 2)]))
    frozen = []
    for name in ("event-reply", "event-copy-result", "owned-result"):
        path = ROOT / "tests/impl250/frozen-R" / name / "source.zil"
        source = path.read_bytes()
        old = ROOT / "evidence/planning/owned-callback-finalR" / name / "source.zil"
        assert source == old.read_bytes()
        frozen.append(dict(name=name, source_sha256=hashlib.sha256(source).hexdigest(),
                           native_if_source_verified=[dict(input="", exit=0, stdout="")]))
    normal = Source(0).owned_error()
    owned_error = [dict(input="", normal=normal,
                        allocator=[dict(fail_at=i, **Source(i).owned_error())
                                   for i in range(normal["alloc_calls"] + 2)])]
    return dict(evidence_level="independent source-operation model; not machine observation",
                frozen=frozen, cases=cases, owned_error=owned_error)


if __name__ == "__main__":
    path = ROOT / "tests/impl250/ORACLE.json"
    path.write_text(json.dumps(build(), indent=2) + "\n")
