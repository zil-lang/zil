#!/usr/bin/env python3
"""Validate authored expectations and fixture metadata. Does NOT execute ZIL."""
from pathlib import Path
import hashlib,json,re,sys
R=Path(__file__).resolve().parents[1];checks=[]
def ck(name,ok,detail=''):checks.append(dict(name=name,pass_=bool(ok),detail=detail))
data=json.loads((R/'conformance/index.json').read_text());cases=data['cases']
rules=set(re.findall(r'<a id="(ZIL-[A-Z]+(?:-[A-Z]+)*)"></a>',(R/'_zil_design.md').read_text()))
tasks={t['id'] for t in json.loads((R/'data/task-index.json').read_text())['tasks']}
diags={d['code']:d for d in json.loads((R/'spec/diagnostics.json').read_text())['entries']}
ck('source corpus nonempty',len(cases)>0)
ck('case IDs and paths unique',len({c['id'] for c in cases})==len(cases) and len({c['path'] for c in cases})==len(cases))
required={'id','path','kind','phase','diagnostic','requirements','acceptance','owner','description','expected','status','source_sha256'}
for c in cases:
    ok=required<=c.keys() and c['owner'] in tasks and set(c['requirements'])<=rules and bool(c['expected'])
    p=(R/c['path']).resolve();ok=ok and p.is_relative_to(R) and p.is_file()
    if ok:ok=bool(p.read_text().strip()) and hashlib.sha256(p.read_bytes()).hexdigest()==c['source_sha256']
    ck(c['id']+' complete and current',ok)
    ck(c['id']+' expectation classified',c['kind'] in ('accept','reject') and c['phase'] in ('parse','expand','types','analyze','run') and ((c['kind']=='accept' and c['diagnostic'] is None) or (c['kind']=='reject' and c['diagnostic'] in diags)))
    ck(c['id']+' not fabricated as executed',c['status']=='NotRun')
    if c['kind']=='reject' and 'error_line' in c['expected']:
        ck(c['id']+' error line exists',1<=c['expected']['error_line']<=len(p.read_text().splitlines()))
    if c['expected'].get('fixture'):
        ck(c['id']+' fixture dependency explicit',c['expected'].get('fixture_status')=='NotImplemented' and (R/'conformance/support/closure-fixtures.md').is_file())
    if c['phase']=='run':
        ck(c['id']+' observable run oracle',('exit_code' in c['expected']) or ('return_value' in c['expected'] and 'entry' in c['expected']))
    if c['diagnostic']:ck(c['id']+' diagnostic stage registered',c['phase'] in diags[c['diagnostic']]['phases'])
covered={v for c in cases for v in c['requirements']}
ck('all surface families represented',{'ZIL-CAPTURE','ZIL-RECEIVER','ZIL-RECORD','ZIL-PIPE','ZIL-SCRIPT'}<=covered)
report={'scope':'authored source expectations and metadata only; zero ZIL compiler executions','cases':len(cases),'accepted_expectations':sum(c['kind']=='accept' for c in cases),'rejected_expectations':sum(c['kind']=='reject' for c in cases),'not_run':len(cases),'fixture_dependent':sum(bool(c['expected'].get('fixture')) for c in cases),'total':len(checks),'passed':sum(c['pass_'] for c in checks),'failures':[c for c in checks if not c['pass_']]}
(R/'evidence/conformance-integrity.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(report,ensure_ascii=False,indent=2));sys.exit(bool(report['failures']))
