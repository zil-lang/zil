#!/usr/bin/env python3
"""Draft source-operation model only. Never reads compiler or machine output."""
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parent


class Failure(Exception):
    pass


@dataclass
class Event:
    mark: str
    allocation: int


@dataclass
class Callback:
    identity: str
    actual_mode: str
    visible_mode: str
    marker: str=""
    allocation: int | None=None
    active: bool=True
    count: int=0
    once_used: bool=False


class Source:
    def __init__(self, fail_at=0):
        self.fail_at=fail_at
        self.calls=self.allocated=self.frees=self.failures=self.peak=0
        self.live=set()
        self.output=[]
        self.forwarded=[]

    def emit(self, value):
        self.output.append(value)

    def allocate(self):
        self.calls+=1
        if self.calls==self.fail_at:
            self.failures+=1
            raise Failure()
        self.live.add(self.calls)
        self.allocated+=1
        self.peak=max(self.peak,len(self.live))
        return self.calls

    def free(self, allocation):
        assert allocation in self.live, "source oracle duplicate destruction"
        self.live.remove(allocation)
        self.frees+=1

    def event(self, mark="A", cancel=False):
        try:
            allocation=self.allocate()
        except Failure:
            self.emit(mark)
            raise
        event=Event(mark,allocation)
        if cancel:
            self.drop_event(event)
            raise Failure()
        return event

    def drop_event(self, event):
        self.emit(event.mark)
        self.free(event.allocation)

    def drop_callback(self, callback):
        assert callback.active, "source oracle drops a transferred/dead F twice"
        if callback.actual_mode=="FnOnce":
            assert not callback.once_used
            self.emit("H")
            self.emit("h")
        else:
            self.emit(callback.marker)
        if callback.allocation is not None:
            self.free(callback.allocation)
        callback.active=False

    def forward(self, callback, bound="FnOnce", nested=False, right=False,
                helper="identity", branch_identity=False):
        assert callback.active
        previous=callback.visible_mode
        assert previous==bound or bound=="FnOnce" or (previous=="Fn" and bound=="FnMut")
        identity=callback.identity
        callback.visible_mode=bound
        if nested:
            prefix="owned_" if nested=="owned" else ""
            self.forward(callback,"FnOnce",helper=prefix+"identity")
            self.forward(callback,"FnOnce",helper=prefix+"branch",
                         branch_identity=(nested=="owned" and right))
        if branch_identity:
            self.forward(callback,"FnOnce",helper="owned_identity")
        self.forwarded.append(dict(identity=identity,inside_bound=bound,
                                   helper=helper,
                                   caller_mode=previous,return_mode=previous,
                                   branch=right if nested else None))
        callback.visible_mode=previous
        assert callback.identity==identity
        return callback

    def marker_call(self, callback):
        assert callback.active and callback.visible_mode=="Fn"
        self.emit(callback.marker.lower())

    def concrete_fn(self,right):
        first=Callback("marker-instance-A","Fn","Fn","A")
        second=Callback("marker-instance-B","Fn","Fn","B")
        current=self.forward(first,nested=True,right=right,helper="nested")
        assert current.identity!=second.identity
        self.marker_call(current)
        self.marker_call(current)
        self.drop_callback(current)
        self.drop_callback(second)

    def copy_fn(self):
        current=self.forward(Callback("prepared-copy-input","Fn","Fn"),nested=True,helper="nested")
        assert current.visible_mode=="Fn"
        # The concrete caller's original Copy value is a separate retained
        # position. Abstract F inside the helper receives no Copy permission.
        for _ in range(3):
            self.emit("C")

    def mutating(self):
        try:
            allocation=self.allocate()
        except Failure:
            self.emit("M")
            raise
        callback=Callback("mutable-instance-M","FnMut","FnMut","M",allocation)
        callback.count+=1
        self.emit(str(callback.count))
        # step's inner identity sees declared FnMut; after its return step
        # returns the same mutated environment to its original caller.
        self.forward(callback)
        for _ in range(2):
            callback.count+=1
            self.emit(str(callback.count))
        assert callback.count==3
        self.drop_callback(callback)

    def once_owned(self,call,cancel):
        try:
            allocation=self.allocate()
        except Failure:
            # No closure yet: reverse local creation, spare h then active H.
            self.emit("h")
            self.emit("H")
            raise
        callback=Callback("once-input-instance","FnOnce","FnOnce",allocation=allocation)
        try:
            side=self.event("D",cancel)
        except Failure:
            # F was the first successfully prepared helper argument.
            self.drop_callback(callback)
            raise
        current=self.forward(callback,nested="owned",right=call,helper="owned_nested")
        # Complete F return is prepared before ordinary companion destruction.
        self.drop_event(side)
        if not call:
            self.drop_callback(current)
            return
        try:
            event=self.event()
        except Failure:
            # FnOnce callee was prepared before the failing explicit argument.
            self.drop_callback(current)
            raise
        assert current.visible_mode=="FnOnce" and not current.once_used
        self.emit("C")
        reply=event
        current.once_used=True
        self.emit("H")
        self.emit("h")
        self.free(current.allocation)
        current.active=False
        self.emit("R")
        self.drop_event(reply)

    def union_data(self,error):
        callback=Callback("union-caller-A","Fn","Fn","A")
        try:
            data=self.event("D")
        except Failure:
            self.drop_callback(callback)
            raise
        current=self.forward(callback)
        # Error Meta{Packet.Data(Event)} and success Packet.Data(Event) each
        # contain exactly one active D; the other ErrorUnion slot is inactive.
        active_slot="error" if error else "success"
        assert active_slot in ("error","success")
        self.drop_event(data)
        self.marker_call(current)
        self.drop_callback(current)

    def owned_repeat(self,mutate):
        callback=Callback("owned-argument-instance-N","FnMut" if mutate else "Fn",
                          "FnMut" if mutate else "Fn","N")
        current=self.forward(callback)
        try:
            for count,mark in enumerate("AB",1):
                event=self.event(mark)
                self.emit(str(count) if mutate else "C")
                self.emit("R")
                self.drop_event(event)
        finally:
            self.drop_callback(current)

    def generic_fn(self):
        callback=Callback("declared-Fn-instance-A","Fn","Fn","A")
        current=self.forward(callback)
        self.marker_call(current)
        self.marker_call(current)
        self.drop_callback(current)

    def product_data(self):
        callback=Callback("product-caller-A","Fn","Fn","A")
        try:
            data=self.event("D")
        except Failure:
            self.drop_callback(callback)
            raise
        current=self.forward(callback,helper="with_product")
        self.drop_event(data)
        self.marker_call(current)
        self.drop_callback(current)

    def run(self,mode):
        self.emit("S")
        exit_code=0
        try:
            if mode in (0,1): self.concrete_fn(mode==1)
            elif mode==2: self.copy_fn()
            elif mode==3: self.mutating()
            elif mode in (4,5,6): self.once_owned(mode!=4,mode==6)
            elif mode in (7,8): self.union_data(mode==7)
            elif mode in (9,10): self.owned_repeat(mode==10)
            elif mode==11: self.generic_fn()
            elif mode==12: self.product_data()
            else: raise AssertionError(mode)
        except Failure:
            exit_code=1
        finally:
            self.emit("Q")
        assert not self.live, "source oracle leaked an allocation"
        return dict(exit=exit_code,stdout="".join(self.output),alloc_calls=self.calls,
                    allocated=self.allocated,frees=self.frees,failures=self.failures,
                    live=0,peak_live=self.peak,forwarded_instance_trace=self.forwarded)


def build():
    cases=[]
    for mode in range(13):
        normal=Source().run(mode)
        cases.append(dict(input=chr(48+mode),normal=normal,
                          allocator_if_source_implemented=[dict(fail_at=i,**Source(i).run(mode))
                                    for i in range(normal['alloc_calls']+2)]))
    original=(ROOT/'frozen-original/source.zil').read_bytes()
    immutable=Path('/workspace/zil/evidence/planning/owned-callback-finalR/f-identity/source.zil').read_bytes()
    assert original==immutable
    return dict(status="Draft oracle only; no native executions",evidence_level="independent source-operation model",
                original_identity_source_sha256=hashlib.sha256(original).hexdigest(),
                original_native_if_implemented=dict(exit=0,stdout=""),cases=cases,
                source_sha256={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest()
                               for p in sorted(ROOT.rglob('*.zil'))})


if __name__=='__main__':
    (ROOT/'ORACLE.json').write_text(json.dumps(build(),indent=2)+'\n')
