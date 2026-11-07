#!/usr/bin/env python3
"""Check document and engineering-record references; does not read the live goal board."""
from pathlib import Path
import hashlib,re,json,sys,runpy
R=Path(__file__).resolve().parents[1];checks=[]
def ck(name,cond,detail=''):
 checks.append({'name':name,'pass':bool(cond),'detail':detail})
def load(p):return json.loads((R/p).read_text())
s=(R/'_zil_design.md').read_text();h=hashlib.sha256(s.encode()).hexdigest();ids=set(re.findall(r'<a id="(ZIL-[A-Z]+(?:-[A-Z]+)*)"></a>',s));gates=set(re.findall(r'\| (V\d\d) \|',s))
ck('all rule anchors unique',len(ids)==len(re.findall(r'<a id="(ZIL-[A-Z]+(?:-[A-Z]+)*)"></a>',s)))
ck('current spec has no revision narrative',not re.search(r'\bS[1-6](?:-[A-Z0-9]+)?\b|本轮|上一版|此前版本|本次测试',s))
ck('current spec describes requirements, not measured claims','只描述终态要求' in s and '不是实现完成' in s)
ck('rule index regenerated',set(load('data/requirement-index.json')['ids'])==ids)
ck('22 stable acceptance groups',len(gates)==22)
contracts=runpy.run_path(str(R/'tools/check-plan.py'))
try:
 items=contracts['read_contracts'](R)
 static_report=contracts['validate'](items,load('spec/interfaces.json'),load('validation/task-test-plans.json'),load('conformance/index.json'),ids,gates)
 ck('static engineering contracts and case registries',static_report['ok'],str(static_report['errors']))
except (OSError,ValueError,TypeError,KeyError) as ex:
 items={}
 ck('static engineering contracts and case registries',False,str(ex))
ck('exactly 14 capability contracts',len(items)==14)
ck('all contracts reference existing rules',all(set(t['requirements'])<=ids for t in items.values()))
ck('all contracts reference existing acceptance',all(set(t['acceptance'])<=gates for t in items.values()))
cover=load('data/requirement-coverage.json')['coverage']
ck('every normative rule has an engineering-contract owner',set(cover)==ids and all(cover.values()) and all(set(keys)<=set(items) for keys in cover.values()))
ck('every acceptance group has contract',gates<={v for t in items.values() for v in t['acceptance']})
ck('contracts reference the single product profile',all(t['profile']=='validation/product-gates.json' for t in items.values()))
d=load('data/designer-data.json');orig=load('data/original-atlas.json');docs=load('data/designer-docs.json')
ck('42 original goals retained exactly',d['goals']==orig['goals'] and len(d['goals'])==42)
ck('28 variable IDs retained',[(x['id'],x['name']) for x in d['variables']]==[(x['id'],x['name']) for x in orig['variables']])
ck('8 language IDs retained',[x['id'] for x in d['languages']]==[x['id'] for x in orig['languages']] and len(d['languages'])==8)
ck('other language routes unchanged',[x for x in d['coverage'] if x['language']!='zil']==[x for x in orig['coverage'] if x['language']!='zil'])
ck('other language settings unchanged',[x for x in d['settings'] if x['language']!='zil']==[x for x in orig['settings'] if x['language']!='zil'])
ck('current ZIL has 42 route records',len([c for c in d['coverage'] if c['language']=='zil'])==42)
ck('current ZIL never marked implemented',all(c['status'] in ['proposal','empirical'] for c in d['coverage'] if c['language']=='zil'))
ck('current routes reference real spec anchors',all(c['specRef'] in ids for c in d['coverage'] if c['language']=='zil'))
rules={x['id'] for x in d['rules']};sources={x['id'] for x in d['sources']}
ck('coverage rules resolve',all(set(c['rules'])<=rules for c in d['coverage']))
ck('coverage sources resolve',all(set(c['sources'])<=sources for c in d['coverage']))
ck('variable options resolve',all(set(x['values'])<=set(o['value'] for o in next(v for v in d['variables'] if v['id']==x['variable'])['options']) for x in d['settings']))
ck('new norm embedded byte-for-byte',docs['spec']==s)
ck('goal/workflow embedded byte-for-byte',docs['goal']==(R/'_zil_goal.md').read_text() and docs['workflow']==(R/'docs/WORKFLOW.md').read_text())
html=(R/'lang-designer.html').read_text();hd=json.loads(re.search(r'<script id="atlas-docs" type="application/json">(.*?)</script>',html,re.S).group(1));ha=json.loads(re.search(r'<script id="atlas-data" type="application/json">(.*?)</script>',html,re.S).group(1))
ck('HTML source payloads equal current data',hd==docs and ha==d)
ck('HTML embeds correct spec digest',d['meta']['specSha256']==h and h[:12] in html)
js=re.search(r'<script>\(\(\) => \{(.*?)</script>',html,re.S).group(1)
ck('no obsolete R2 UI label',not re.search(r'\bR2\b|\br2\b|language-atlas-d2',js))
ck('no external script or stylesheet request',not re.search(r'<script[^>]+src=|<link[^>]+(?:stylesheet|href=)',html,re.I))
ck('updated docs nav includes workflow and syntax',all(v in js for v in ["['goal','施工总计划']","['workflow','协作规程']","['syntax','语法与属性']"]))
ck('attributes catalogue gives deterministic phase',all(isinstance(x['phase'],int) and x['targets'] and x['repeat'] for x in load('spec/attributes.json')['attributes']))
# Grammar reference hygiene only, not a claim of parsing or ambiguity proof.
g=(R/'spec/grammar.ebnf').read_text();g=re.sub(r'\(\*.*?\*\)','',g,flags=re.S);defs=set(re.findall(r'^([A-Za-z][A-Za-z0-9]*)\s*=',g,re.M));q=re.sub(r'"[^"]*"','',g);names=set(re.findall(r'\b[A-Za-z][A-Za-z0-9]*\b',q));terminal={'IDENT','STRING','CHAR','NUMBER','LIFEIDENT','NEWLINE','NonDelimiterToken','TUPLEINDEX'}
ck('grammar references defined nonterminals',names<=defs|terminal,','.join(sorted(names-defs-terminal)))
ck('grammar # alternatives present',all(x in g for x in ['Attribute =','MetaEval =','MetaBlock =','MetaEmit =','GenericParameter =']))
ck('input hashes correspond mounted inputs',all(not (Path('/mnt/data')/n).exists() or hashlib.sha256((Path('/mnt/data')/n).read_bytes()).hexdigest()==v for n,v in load('data/input-hashes.json').items()))
report={'scope':'Document/data/engineering-record reference integrity only; does not read the live goal board or check all development branches. Not compiler semantic acceptance','live_plan_checked':False,'development_branches_checked':False,'contract_source':'validation/engineering-contracts.json','total':len(checks),'passed':sum(c['pass'] for c in checks),'checks':checks}
(R/'evidence').mkdir(parents=True, exist_ok=True)
(R/'evidence/document-checks.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(json.dumps(report,ensure_ascii=False,indent=2))
sys.exit(0 if report['total']==report['passed'] else 1)
