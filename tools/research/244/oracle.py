#!/usr/bin/env python3
"""Source-operation oracle for 244. Never reads compiler or machine output."""
from dataclasses import dataclass
import json
from pathlib import Path


@dataclass
class Mark:
    code: int


@dataclass
class Items:
    store: int | None
    leaves: list[tuple[Mark, int]]


@dataclass
class Session:
    subscriptions: Items
    nodes: Items


@dataclass
class Fault:
    partial: Items


@dataclass
class Phase:
    tag: str
    payload: object = None


@dataclass
class App:
    phase: Phase | None


class Failure(Exception):
    def __init__(self, value: Fault):
        super().__init__("source ? propagation")
        self.value = value


class Machine:
    def __init__(self, fail_at: int = 0):
        self.fail_at = fail_at
        self.calls = self.allocated = self.frees = self.failures = self.peak = 0
        self.live: set[int] = set()
        self.output: list[str] = []
        self.scope: list[object | None] = []

    def emit(self, code: int):
        self.output.append(chr(code))

    def request(self) -> int | None:
        self.calls += 1
        if self.calls == self.fail_at:
            self.failures += 1
            return None
        allocation = self.calls
        assert allocation not in self.live
        self.live.add(allocation)
        self.allocated += 1
        self.peak = max(self.peak, len(self.live))
        return allocation

    def free(self, allocation: int | None):
        if allocation is not None:
            assert allocation in self.live, "oracle double free"
            self.live.remove(allocation)
            self.frees += 1

    def drop(self, value):
        if value is None:
            return
        if isinstance(value, Mark):
            self.emit(value.code)
        elif isinstance(value, Items):
            for mark, buffer in value.leaves:
                self.drop(mark)
                self.free(buffer)
            self.free(value.store)
        elif isinstance(value, Session):
            self.drop(value.subscriptions)
            self.drop(value.nodes)
        elif isinstance(value, Fault):
            self.drop(value.partial)
            self.emit(69)
        elif isinstance(value, App):
            self.drop(value.phase)
        elif isinstance(value, Phase):
            if value.tag in ("ready", "loaded"):
                self.drop(value.payload)
            elif value.tag == "nested":
                first, right, _ = value.payload
                self.drop(first)
                for mark in right:
                    self.drop(mark)
            else:
                assert value.tag == "empty"
        else:
            assert value == "defer-Q"
            self.emit(81)

    def cleanup(self):
        for value in reversed(self.scope):
            self.drop(value)
        self.scope.clear()

    def set(self, count: int, base: int) -> Items:
        store = self.request() if count else None
        items = Items(store, [])
        if count and store is None:
            raise Failure(Fault(items))
        for index in range(count):
            mark = Mark(base + index)
            buffer = self.request()
            if buffer is None:
                # Return prepares Fault from the existing Store. The current
                # loose Mark is then cleaned before that error leaves set().
                self.drop(mark)
                raise Failure(Fault(items))
            items.leaves.append((mark, buffer))
        return items

    def session(self, count: int, fail: bool, left: int, right: int) -> Session:
        subscriptions = self.set(count, left)
        if fail:
            raise Failure(Fault(subscriptions))
        try:
            nodes = self.set(count, right)
        except Failure:
            self.drop(subscriptions)
            raise
        return Session(subscriptions, nodes)

    def ready(self, count: int, fail=False, left=97, right=65) -> App:
        return App(Phase("ready", self.session(count, fail, left, right)))

    def body(self, count: int, mode: int):
        if mode == 7:
            self.scope.append(Phase("ready", self.session(count, False, 97, 65)))
            self.scope.extend(("defer-Q", Mark(76)))
            prepared = Phase("ready", self.session(count, False, 109, 77))
            self.drop(self.scope[0])
            self.scope[0] = prepared  # preserve the original lexical shell age
            self.emit(78)
            return
        if mode == 0:
            app = App(Phase("empty"))
        elif mode == 2:
            try:
                payload = self.session(count, True, 97, 65)
            except Failure as error:
                payload = error.value
            app = App(Phase("loaded", payload))
        elif mode == 5:
            app = App(Phase("nested", (Mark(97), [] if count == 0 else [Mark(99), Mark(100)], count)))
        else:
            app = self.ready(count)
        self.scope.extend((app, "defer-Q", Mark(76)))
        if mode == 3:
            prepared = self.ready(count, True, 109, 77)
            self.drop(self.scope[0])
            self.scope[0] = app = prepared
        if mode == 4:
            # relay consumes and returns the same complete App. No old owner
            # remains to clean at commit, and its original slot age is kept.
            self.scope[0] = None
            self.scope[0] = app
        if mode == 6:
            self.emit(78)
            return
        phase = app.phase
        app.phase = None
        if phase.tag == "empty":
            self.emit(69)
        elif phase.tag == "ready":
            self.emit(82)
            self.drop(phase.payload)  # match-arm local, after the body marker
        elif phase.tag == "loaded":
            if isinstance(phase.payload, Fault):
                raise Failure(phase.payload)
            self.emit(82)
            self.drop(phase.payload)
        else:
            first, right, copy = phase.payload
            self.emit(69 if not right else 66 if copy == 1 else 67)
            # All pattern tests precede ownership transfer. Reverse recursive
            # publication gives the declared a,c,d cleanup order at arm exit.
            self.drop(first)
            for mark in right:
                self.drop(mark)
        self.emit(78)

    def result(self, exit_code: int):
        assert not self.live, "oracle leaked an actual allocation"
        assert self.allocated == self.frees
        return dict(exit=exit_code, stdout="".join(self.output), alloc_calls=self.calls,
                    allocated=self.allocated, frees=self.frees, failures=self.failures,
                    live=0, peak_live=self.peak)

    def run(self, data: bytes):
        if len(data) < 2 or not 48 <= data[0] <= 51 or not 48 <= data[1] <= 55:
            return self.result(9)
        self.emit(83)
        try:
            self.body(data[0] - 48, data[1] - 48)
        except Failure as error:
            self.cleanup()
            self.drop(error.value)  # native error boundary, after main cleanup
            return self.result(1)
        self.cleanup()
        return self.result(0)


def matrix():
    cases = []
    for count in range(4):
        for mode in range(8):
            data = f"{count}{mode}".encode()
            normal = Machine().run(data)
            probes = [{"fail_at": fail, **Machine(fail).run(data)}
                      for fail in range(normal["alloc_calls"] + 2)]
            cases.append(dict(input=data.decode(), normal=normal, allocator=probes))
    invalid = [{"input_hex": data.hex(), **Machine().run(data)}
               for data in (b"", b"0", b"40", b"18", b"x0")]
    return dict(status="PredeclaredBeforeConsumerExecution", optimizations=["O0", "O2"],
                source_model="Named source operations; no compiler/IR/machine observations",
                bytes="Observed only: Store element layout is not guessed as a byte-cost oracle",
                cases=cases, invalid=invalid)


if __name__ == "__main__":
    target = Path(__file__).resolve().parents[3] / "tests/impl244/ORACLE.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(matrix(), indent=2) + "\n")
    print(target)
