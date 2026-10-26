#!/usr/bin/env python3
"""Actual CLI failures must classify correctly and publish no executable."""
import json,os,pathlib,subprocess,tempfile
ROOT=pathlib.Path(__file__).resolve().parents[2]
BIN=pathlib.Path(os.environ.get('ZIL_BIN',ROOT/'target/debug/zil'))
# Nested addresses preserve the Unsupported category; immutable FnMut calls
# remain exact SafeReject coverage in closures_frontend::c014_06.
checks=[('syntax-error.zil','SafeReject','parse',None),('type-error.zil','SafeReject','types','E-TYPE-MISMATCH'),('unsupported.zil','Unsupported','types','E-UNSUPPORTED')]
assertions=0;observed=[]
with tempfile.TemporaryDirectory(prefix='zil-failure-') as temp:
    for i,(name,status,phase,code) in enumerate(checks):
        output=pathlib.Path(temp)/f'program-{i}'
        cmd=[str(BIN),'build',str(ROOT/'examples/bootstrap'/name),'--json','--output',str(output)]
        p=subprocess.run(cmd,text=True,capture_output=True,timeout=30)
        d=json.loads(p.stdout)
        assert p.returncode!=0,(name,p.stdout,p.stderr);assertions+=1
        assert d['status']==status,(name,d);assertions+=1
        assert any(x['phase']==phase and (code is None or x['code']==code) for x in d['diagnostics']),d;assertions+=1
        assert not output.exists(),name;assertions+=1
        observed.append({'source':name,'status':status,'phase':phase,'codes':[x['code'] for x in d['diagnostics']],'exit_code':p.returncode})
print(json.dumps({'executed':len(checks),'assertions':assertions,'evidence_level':'NativeCliFailureClassification','observed':observed}))
