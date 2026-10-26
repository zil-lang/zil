#!/usr/bin/env python3
"""Exercise the runner's own failure states; these are not language acceptances."""
import importlib.util,json,pathlib,subprocess,sys,tempfile
ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('zil_runner',ROOT/'tools/run-cases.py');runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
(ROOT/'.work').mkdir(exist_ok=True)
executed=0;assertions=0
with tempfile.TemporaryDirectory(prefix='runner-faults-',dir=ROOT/'.work') as temporary:
    root=pathlib.Path(temporary);rel=root.relative_to(ROOT)
    cmd=[sys.executable,str(ROOT/'tools/run-cases.py'),'--task','NO-SUCH-TASK','--manifest','validation/task-test-plans.json','--out-root',str(rel/'empty')]
    p=subprocess.run(cmd,capture_output=True,text=True,timeout=10,cwd=ROOT)
    assert p.returncode!=0 and json.loads(p.stdout)['selected']==0;executed+=1;assertions+=1
    for name,fixture in [('missing',None),('empty',{'steps':[]}),('zero',{'steps':[{'kind':'command','command':['{python}','-c','print(\'{"executed":0,"assertions":0}\')'],'result_json':True}]}),('no-assertions',{'steps':[{'kind':'command','command':['{python}','-c','print(\'{"executed":1,"assertions":0}\')'],'result_json':True}]}),('unsupported',{'steps':[{'kind':'accept','source':'examples/bootstrap/unsupported.zil','phase':'verify'}]}),('reported-failure',{'steps':[{'kind':'command','command':['{python}','-c','print(\'{"executed":1,"assertions":1,"status":"Fail"}\')'],'result_json':True}]})]:
        f=root/f'{name}.json'
        if fixture is not None:f.write_text(json.dumps(fixture))
        c={'id':'POLICY-'+name,'required_fixture':str(f.relative_to(ROOT))}
        result=runner.run_case(c,root,False)
        assert result['status']=='Fail',result;executed+=1;assertions+=1
    try:runner.execute([sys.executable,'-c','import time; time.sleep(0.2)'],root/'timeout',timeout=.01)
    except runner.CaseFailure as ex:assert 'timeout' in str(ex);executed+=1;assertions+=1
    else:raise AssertionError('timeout passed')
    try:runner.inside('../outside')
    except runner.CaseFailure:executed+=1;assertions+=1
    else:raise AssertionError('path escape accepted')
print(json.dumps({'executed':executed,'assertions':assertions,'evidence_level':'HarnessPolicy'}))
