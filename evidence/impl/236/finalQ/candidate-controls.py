#!/usr/bin/env python3
"""Current-candidate controls. Baseline controls.py intentionally expects Unsupported.
Uses original leaf/defer ZIL, real LLVM API objects and existing allocation shim.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
sys.dont_write_bytecode = True


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo', type=Path, required=True)
    p.add_argument('--cli', type=Path, required=True)
    p.add_argument('--compiler-build-receipt', type=Path, required=True)
    p.add_argument('--clang', required=True)
    p.add_argument('--out', type=Path, required=True)
    a=p.parse_args()
    repo,cli,out=a.repo.resolve(),a.cli.resolve(),a.out.resolve()
    spec=importlib.util.spec_from_file_location('runner236',repo/'tools/language/236/run.py')
    helper=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    command,digest,save=helper.command,helper.digest,helper.save
    receipt=json.loads(a.compiler_build_receipt.read_text())
    commit=subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()
    dirty=subprocess.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip()
    if (dirty or receipt['status']!='Pass' or receipt['exit_code']!=0 or receipt['git_status']
            or receipt['source_commit']!=commit or receipt['cli_sha256']!=digest(cli)):
        raise RuntimeError('current candidate build identity not established')
    manifest=json.loads((repo/'tools/language/236/SOURCE-MANIFEST.json').read_text())
    for name,expected in manifest['files'].items():
        if digest(repo/name)!=expected:
            raise RuntimeError('consumer input changed: '+name)
    out.mkdir(parents=True,exist_ok=False)
    report={'status':'Running','scope':'current-candidate existing-shape and no-allocation defer controls',
            'candidate_commit':commit,'cli_sha256':digest(cli),
            'script_sha256':digest(Path(__file__)), 'builds':[], 'observations':[]}
    save(out/'RESULT.json',report)
    try:
        for opt in ['O0','O2']:
            for name,module_count,stdout in [('leaf_control',3,b'A'),('defer_control',2,b'Q')]:
                directory=out/opt/name
                directory.mkdir(parents=True)
                captured,wrapper=directory/'api.o',directory/'capture-object.sh'
                wrapper.write_text('#!/bin/sh\nset -eu\nfor arg in "$@"; do\ncase "$arg" in *.o|*.obj) cp -- "$arg" '
                                   +shlex.quote(str(captured))+' ;; esac\ndone\nexec '+shlex.quote(a.clang)+' "$@"\n')
                wrapper.chmod(0o700)
                binary=directory/'program'
                result=command([cli,'build',repo/f'examples/mainline/zui_core236/{name}.zil','--backend','llvm-api',
                                '--opt',opt,'--clang',wrapper,'-o',binary,'--emit-dir',directory/'phases','--json'],directory/'build')
                built=json.loads(result.stdout)
                if (result.returncode or built['status']!='Success' or built['verification_runs']!=1
                        or len(built['source_files'])!=module_count or not captured.is_file()):
                    raise RuntimeError(f'{opt}/{name}: control build failed: {built}')
                native=built['native_codegen']
                if (native['backend']!='llvm-api' or native['fallback'] is not None or native['actual_units']!=1
                        or not native['object_reports']
                        or any(r['input_kind']!='builder' or not r['verify_before_after'] for r in native['object_reports'])):
                    raise RuntimeError('control object identity differs')
                ordinary=command([binary],directory/'ordinary')
                if ordinary.returncode or ordinary.stdout!=stdout or ordinary.stderr:
                    raise RuntimeError('ordinary control output differs')
                report['observations'].append({'opt':opt,'name':name,'instrumented':False,'exit':0,'stdout':stdout.decode()})
                probe=directory/'probe'
                linked=command([a.clang,captured,repo/'tools/language/173/allocator_probe.c',
                                '-Wl,--wrap=realloc','-Wl,--wrap=free','-o',probe],directory/'probe-link')
                if linked.returncode:
                    raise RuntimeError('real control object probe link failed')
                for fail in ([0,1,2] if name=='leaf_control' else [0,1]):
                    failed=name=='leaf_control' and fail==1
                    actual=command([probe],directory/f'fail-{fail}',environment={'ZIL_TEST_FAIL_AT':str(fail)})
                    expected_stdout=b'FA' if failed else stdout
                    if actual.returncode!=int(failed) or actual.stdout!=expected_stdout:
                        raise RuntimeError('instrumented control output differs')
                    stats=json.loads(actual.stderr)
                    leaf=name=='leaf_control'
                    expected={'alloc_calls':int(leaf),'allocated':int(leaf and not failed),
                              'frees':int(leaf and not failed),'failures':int(failed),
                              'live':0,'peak_live':int(leaf and not failed)}
                    if any(stats.get(k)!=v for k,v in expected.items()):
                        raise RuntimeError(f'control allocation ABI differs: {stats}, expected {expected}')
                    report['observations'].append({'opt':opt,'name':name,'instrumented':True,'fail_at':fail,
                                                   'exit':actual.returncode,'stdout':actual.stdout.decode(),'allocation':stats})
                report['builds'].append({'opt':opt,'name':name,'verification_runs':1,'module_count':module_count,
                                         'api_object_sha256':digest(captured),'program_sha256':digest(binary)})
        after=subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()
        dirty=subprocess.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip()
        if after!=commit or dirty or digest(cli)!=report['cli_sha256']:
            raise RuntimeError('control candidate identity changed')
        report.update(status='Pass',ordinary_executions=4,allocator_executions=10,
                      actual_allocation_failures=sum(r.get('allocation',{}).get('failures',0) for r in report['observations']))
    except Exception as error:
        report.update(status='Fail',error=str(error))
        raise
    finally:
        save(out/'RESULT.json',report)
        save(out/'ARTIFACT-MANIFEST.json',{'sha256':{str(f.relative_to(out)):digest(f) for f in sorted(out.rglob('*')) if f.is_file()}})

if __name__=='__main__':
    main()
