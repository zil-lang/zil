#!/usr/bin/env python3
"""Root independent CLI/native replay; expectations derived from source, not oracle.py."""
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys

repo, cli, out = map(Path, sys.argv[1:])
out.mkdir(parents=True, exist_ok=False)
def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()
def record(cmd, label, *, data=None, env=None):
    r = subprocess.run(list(map(str, cmd)), input=data, capture_output=True, env=env, timeout=90)
    (out / (label + '.stdout')).write_bytes(r.stdout)
    (out / (label + '.stderr')).write_bytes(r.stderr)
    (out / (label + '.command.json')).write_text(json.dumps(list(map(str, cmd))))
    (out / (label + '.exit')).write_text(str(r.returncode))
    return r

normal = []
for n in range(4):
    a, b = 'abc'[:n], 'ABC'[:n]
    m, M = 'mno'[:n], 'MNO'[:n]
    pattern = 'Ea' if n == 0 else ('Bacd' if n == 1 else 'Cacd')
    outputs = ['SENLQ', 'SR'+a+b+'NLQ', 'SLQ'+a+'E', 'SLQ'+a+b+m+'E',
               'SR'+a+b+'NLQ', 'S'+pattern+'NLQ', 'SNLQ'+a+b, 'S'+a+b+'NLQ'+m+M]
    for mode, stdout in enumerate(outputs):
        normal.append((f'{n}{mode}'.encode(), stdout.encode(), 1 if mode in (2,3) else 0))
invalid = [(x,b'',9) for x in (b'', b'0', b'40', b'18', b'x0')]
failures = [('27',1,'SE',0), ('27',2,'SaE',1), ('27',3,'SbaE',2),
            ('27',4,'SabE',3), ('27',5,'SAabE',4), ('27',7,'SLQabABE',6),
            ('27',8,'SmLQabABE',7), ('27',10,'SmnLQabABE',9),
            ('27',11,'SMmnLQabABE',10), ('27',12,'SNmnLQabABME',11),
            ('22',1,'SLQE',0), ('22',2,'SaLQE',1), ('22',3,'SbLQaE',2)]
# These expectations are saved before compiling or observing executables.
(out/'PREDECLARED.json').write_text(json.dumps({
    'normal_and_invalid': [(d.hex(),s.decode(),e) for d,s,e in normal+invalid],
    'failures': failures, 'source_oracle_imported': False}, indent=2)+'\n')
clang=Path(os.environ['ZIL_CLANG'])
for opt in ('O0','O2'):
    build = out / opt
    build.mkdir()
    wrapper = build/'capture.sh'
    wrapper.write_text('#!/bin/sh\nset -eu\nfor arg in "$@"; do\ncase "$arg" in *.o|*.obj) cp -- "$arg" '+shlex.quote(str(build/'api.o'))+' ;; esac\ndone\nexec '+shlex.quote(str(clang))+' "$@"\n')
    wrapper.chmod(0o700)
    r=record([cli,'build',repo/'tests/impl244/project/main.zil','--backend','llvm-api',
              '--opt',opt,'--clang',wrapper,'-o',build/'program','--emit-dir',build/'phases','--json'], opt+'-build')
    report=json.loads(r.stdout)
    assert r.returncode==0 and report['status']=='Success' and report['phase']=='link', report
    assert report['verification_runs']==1
    native=report['native_codegen']
    assert native['backend']=='llvm-api' and native['fallback'] is None
    assert native['actual_units']==1
    assert all(x['input_kind']=='builder' and x['parse_ns']==0 and x['verify_before_after'] for x in native['object_reports'])
    for i,(data,stdout,exitcode) in enumerate(normal+invalid):
        r=record([build/'program'],f'{opt}-normal-{i}',data=data)
        assert (r.returncode,r.stdout,r.stderr)==(exitcode,stdout,b''),(opt,data,r)
    r=record([clang,build/'api.o',repo/'tools/language/173/allocator_probe.c',
              '-Wl,--wrap=realloc','-Wl,--wrap=free','-o',build/'probe'],opt+'-link-probe')
    assert r.returncode==0
    for data,fail,stdout,count in failures:
        env={**os.environ,'ZIL_TEST_FAIL_AT':str(fail)}
        r=record([build/'probe'],f'{opt}-fault-{data}-{fail}',data=data.encode(),env=env)
        assert r.returncode==1 and r.stdout==stdout.encode(),(opt,data,fail,r)
        counters=json.loads(r.stderr)
        expected=dict(alloc_calls=fail,allocated=count,frees=count,failures=1,live=0,peak_live=count)
        assert all(counters[k]==v for k,v in expected.items()),(data,fail,counters,expected)
result={
    'reviewer':'/root','verdict':'PassWithinScope',
    'scope':'244 independent source-derived CLI/native and selected real allocation failures',
    'implementation_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),
    'cli_sha256':digest(cli),'api_builds':2,'normal_invalid_runs':74,
    'real_allocation_failure_runs':26,'total_native_runs':100,'all_live_zero':True,
    'test_oracle_imported':False,'predeclared_sha256':digest(out/'PREDECLARED.json'),
    'source_hashes':{str(p.relative_to(repo)):digest(p) for p in sorted((repo/'tests/impl244/project').glob('*.zil'))},
}
(out/'RESULT.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
