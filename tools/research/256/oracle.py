#!/usr/bin/env python3
"""256 source expectations; never reads compiler reports or machine output."""
import importlib.util
import json
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[3]
FROZEN=ROOT/'tests/impl256/frozen-prep'


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
                frozen=frozen,cases=cases,extensions=[])


if __name__=='__main__':
    (ROOT/'tests/impl256/ORACLE.json').write_text(json.dumps(build(),indent=2)+'\n')
