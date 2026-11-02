#!/usr/bin/env python3
"""Read-only current-CLI checks. All products stay in this draft directory."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parent
CLI=Path('/workspace/.zil-env/target-core/debug/zil')
BUILD='fa8638df7e9fc30e717e23191252bb1600d1e42f'
R='288e29628389d83a335783d5488be39f22b2008b'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=sha(CLI)
sources=[('original',ROOT/'frozen-original/source.zil',False),
         ('project',ROOT/'project/main.zil',False)]
sources += [('probe-'+p.stem,p,False) for p in sorted((ROOT/'probes').glob('*.zil'))]
sources += [('negative-'+p.stem,p,True) for p in sorted((ROOT/'negative').glob('*.zil'))]
rows=[]
for name,source,negative in sources:
    stages=[]
    for stage in ('parse','verify'):
        out=ROOT/'baseline'/name/stage
        out.mkdir(parents=True,exist_ok=True)
        command=[str(CLI),'check',str(source),'--stop-after',stage,'--json','--emit-dir',str(out/'phases')]
        result=subprocess.run(command,capture_output=True,timeout=30)
        (out/'stdout.json').write_bytes(result.stdout)
        (out/'stderr').write_bytes(result.stderr)
        (out/'exit').write_text(str(result.returncode)+'\n')
        report=json.loads(result.stdout)
        stages.append(dict(command=command,exit_code=result.returncode,
                           status=report['status'],phase=report['phase'],
                           diagnostics=report['diagnostics'],verification_runs=report['verification_runs'],
                           source_files=report['source_files'],
                           report=str((out/'stdout.json').relative_to(ROOT))))
    rows.append(dict(case=name,target_class='negative' if negative else 'new-positive',
                     source_sha256=sha(source),stages=stages,
                     interpretation='BlockedByMissingPositiveCapability; not correct semantic rejection evidence' if negative else 'Unsupported baseline; not Pass'))
assert sha(CLI)==before,'CLI changed during baseline collection'
receipt=dict(status='Draft baseline; no feature acceptance',binary=str(CLI),binary_sha256=before,
             binary_build_commit=BUILD,production_inputs_equal_tested_R=R,
             identity_basis='Root attested equality of crates/*/src, build.rs, Cargo manifests/lock and toolchain from build commit to tested R. Current main is not used as binary build identity.',
             actual_cli_invocations=sum(len(r['stages']) for r in rows),
             new_positive_sources=sum(r['target_class']=='new-positive' for r in rows),
             negative_sources=sum(r['target_class']=='negative' for r in rows),
             verification_runs=sum(s['verification_runs'] for r in rows for s in r['stages']),
             native_runs=0,accepted_new_feature_cases=0,rows=rows,
             environment_invocation_errors=[dict(command='target-core/debug/zil check --help without sourcing env.sh',category='Infrastructure invocation error',detail='libLLVM.so.23.1 loader path absent',counted_as_compiler_execution=False)],
             usage_probe_errors=[dict(command='zil check --help after sourcing env.sh',category='CLI usage error',detail='help is a top-level option',counted_as_source_check=False)])
(ROOT/'BASELINE.json').write_text(json.dumps(receipt,indent=2)+'\n')
for row in rows:
    print(row['case'],[(s['status'],s['phase']) for s in row['stages']])
