#!/usr/bin/env python3
"""Regenerate dispatch/input indexes. Never marks compiler expectations as executed."""
from pathlib import Path
import hashlib,json,re
R=Path(__file__).resolve().parents[1]
def dump(p,obj):
    (R/p).write_text(json.dumps(obj,ensure_ascii=False,indent=2)+'\n')
def sha(p):return hashlib.sha256((R/p).read_bytes()).hexdigest()
spec=(R/'_zil_design.md').read_text()
rules=re.findall(r'<a id="(ZIL-[A-Z]+(?:-[A-Z]+)*)"></a>',spec)
cases=[]
for group in ('closure','receiver','record','pipe','script'):
    cases.extend(json.loads((R/'conformance'/f'{group}.json').read_text())['cases'])
assert len({c['id'] for c in cases})==len(cases)
for c in cases:c['source_sha256']=sha(c['path'])
dump('conformance/index.json',{'schema_version':1,'generated_from':['conformance/'+g+'.json' for g in ('closure','receiver','record','pipe','script')],'execution_state':'NotRun','cases':cases})
# The registry also contains implemented diagnostics outside conformance cases.
# Cases enrich the explicit registry; regeneration must not delete those codes.
registered=json.loads((R/'spec/diagnostics.json').read_text())
diagnostics={d['code']:d for d in registered['entries']}
assert len(diagnostics)==len(registered['entries']), 'duplicate diagnostic code'
for c in cases:
    if c['diagnostic']:
        d=diagnostics.setdefault(c['diagnostic'],{'code':c['diagnostic'],'phases':[],'requirements':[]})
        d['phases']=sorted(set(d['phases']+[c['phase']]))
        d['requirements']=sorted(set(d['requirements']+c['requirements']))
dump('spec/diagnostics.json',{'schema_version':1,'purpose':'Explicit stable diagnostic registry enriched by conformance mappings; safety meaning is defined only by normative rule IDs','entries':sorted(diagnostics.values(),key=lambda d:d['code'])})
inputs=['_zil_design.md','spec/grammar.ebnf','spec/attributes.json','spec/diagnostics.json','spec/interfaces.json','policy/dependencies.json','validation/product-gates.json']
files={p:sha(p) for p in inputs}
canonical=''.join(p+'\0'+h+'\n' for p,h in sorted(files.items()))
bundle=hashlib.sha256(canonical.encode()).hexdigest()
dump('data/normative-inputs.json',{'schema_version':1,'algorithm':'sha256 of sorted path + NUL + sha256 + LF entries','files':files,'normative_hash':bundle})
contracts=json.loads((R/'validation/engineering-contracts.json').read_text())['contracts']
if len(contracts)!=14 or any(not c.get('requirements') or not c.get('done_when') for c in contracts.values()):
    raise ValueError('static engineering contracts missing or incomplete')
dump('data/requirement-index.json',{'generated_from':'_zil_design.md','ids':rules})
coverage={i:[key for key,c in contracts.items() if i in c['requirements']] for i in rules}
if not all(coverage.values()):
    raise ValueError('normative rule without engineering contract')
dump('data/requirement-coverage.json',{'generated_from':'validation/engineering-contracts.json','coverage':coverage})
print(json.dumps({'contracts':len(contracts),'rules':len(rules),'source_cases':len(cases),'diagnostics':len(diagnostics),'normative_hash':bundle},ensure_ascii=False))
