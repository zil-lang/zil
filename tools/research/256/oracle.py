#!/usr/bin/env python3
"""256 source expectations; never reads compiler reports or machine output."""
import importlib.util
import json
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[3]
FROZEN=ROOT/'tests/impl256/frozen-prep'


def extensions(legacy):
    class Source(legacy.Source):
        def items(self,base,cancel=False):
            store=self.allocate()
            leaves=[]
            try:
                for label in (chr(base),chr(base+1)):
                    leaves.append(self.event(label))
            except legacy.Failure:
                self.drop_items((store,leaves))
                raise
            value=(store,leaves)
            if cancel:
                self.drop_items(value)
                raise legacy.Failure()
            return value

        def drop_items(self,value):
            store,leaves=value
            for leaf in leaves:self.drop_event(leaf)
            self.free(store)

        def companion(self,kind):
            mode='FnMut' if kind.startswith('address') else 'Fn'
            callback=legacy.Callback('extended-marker-A',mode,mode,'A')
            pointer=40
            def edit():
                nonlocal pointer
                callback.count+=1
                self.emit(str(callback.count))
                pointer+=callback.count
                return pointer
            first=None
            if kind=='address-after-call':first=edit()
            try:
                value=self.items(68)
            except legacy.Failure:
                self.drop_callback(callback)
                raise
            self.forward(callback,mode if kind in ('store','slice') or mode=='FnMut' else 'FnOnce')
            self.drop_items(value)
            if kind.startswith('address'):
                a=first if first is not None else edit()
                b=edit()
                assert a+b==84 and pointer==43 and callback.count==2
            elif kind=='slice':
                for _ in range(2):
                    self.emit('C')
                    assert sum((7,9))==16
            else:
                self.marker_call(callback)
                if kind=='store':self.marker_call(callback)
            self.drop_callback(callback)

        def loop_forward(self,count):
            callback=legacy.Callback('loop-forward-A','Fn','Fn','A')
            prior=callback.visible_mode
            callback.visible_mode='FnOnce'
            for _ in range(count):self.forward(callback)
            callback.visible_mode=prior
            self.marker_call(callback)
            self.marker_call(callback)
            self.drop_callback(callback)

        def loop_mutating(self,count):
            callback=legacy.Callback('loop-mutation-A','FnMut','FnMut','A')
            for _ in range(count+1):
                callback.count+=1
                self.emit(str(callback.count))
            assert callback.count==count+1
            self.drop_callback(callback)

        def later(self,cancel):
            callback=legacy.Callback('middle-parameter-once','FnOnce','FnOnce')
            try:early=self.items(68)
            except legacy.Failure:
                self.drop_callback(callback)
                raise
            try:late=self.items(76,cancel)
            except legacy.Failure:
                # Successful arguments are early, then F. Later's failed
                # partial/full preparation is already destroyed by items().
                self.drop_callback(callback)
                self.drop_items(early)
                raise
            self.forward(callback)
            # Parameter storage is destroyed in reverse registration order;
            # returned F is no longer owned by its formal parameter slot.
            self.drop_items(late)
            self.drop_items(early)
            self.emit('C')
            callback.once_used=True
            callback.active=False
            self.emit('H')
            self.emit('h')

        def run_extension(self,mode):
            self.emit('S')
            exit_code=0
            try:
                kinds=['store','address','address-after-call','slice','union-error','union-success']
                if mode<6:self.companion(kinds[mode])
                elif mode in (6,7):self.loop_forward(0 if mode==6 else 3)
                elif mode in (8,9):self.loop_mutating(0 if mode==8 else 3)
                elif mode in (10,11):self.later(mode==10)
                else:raise AssertionError(mode)
            except legacy.Failure:exit_code=1
            finally:self.emit('Q')
            assert not self.live
            return dict(exit=exit_code,stdout=''.join(self.output),alloc_calls=self.calls,
                        allocated=self.allocated,frees=self.frees,failures=self.failures,
                        live=0,peak_live=self.peak,forwarded_instance_trace=self.forwarded)
    result=[]
    for mode in range(12):
        normal=Source().run_extension(mode)
        result.append(dict(input=chr(48+mode),normal=normal,
                           allocator=[dict(fail_at=i,**Source(i).run_extension(mode))
                                      for i in range(normal['alloc_calls']+2)]))
    return result


def build():
    preserved=json.loads((FROZEN/'ORACLE.json').read_text())
    spec=importlib.util.spec_from_file_location('impl256_frozen_source_oracle',FROZEN/'oracle.py')
    module=importlib.util.module_from_spec(spec)
    sys.modules[spec.name]=module
    spec.loader.exec_module(module)
    cases=[]
    for mode,case in enumerate(preserved['cases']):
        assert module.Source().run(mode)==case['normal']
        for expected in case['allocator_if_source_implemented']:
            actual=module.Source(expected['fail_at']).run(mode)
            assert actual=={k:v for k,v in expected.items() if k!='fail_at'}
        cases.append(dict(input=case['input'],normal=case['normal'],
                          allocator=case['allocator_if_source_implemented']))
    frozen=[dict(name='original-identity',source='frozen-prep/frozen-original/source.zil',
                 native_if_source_verified=[dict(input='',exit=0,stdout='')])]
    for path in sorted((FROZEN/'probes').glob('*.zil')):
        frozen.append(dict(name=path.stem,source='frozen-prep/probes/'+path.name,
                           native_if_source_verified=[dict(input='',exit=0,stdout='')]))
    return dict(evidence_level='independent frozen source-operation oracle; no machine observations',
                frozen=frozen,cases=cases,extensions=extensions(module))


if __name__=='__main__':
    (ROOT/'tests/impl256/ORACLE.json').write_text(json.dumps(build(),indent=2)+'\n')
