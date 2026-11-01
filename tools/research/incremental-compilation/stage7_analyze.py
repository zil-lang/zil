#!/usr/bin/env python3
"""Summarize every paired cell; invalid or incomplete data fails rather than disappearing."""
import argparse, json, math, statistics
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('directory',type=Path)
p.add_argument('--case',action='append',help='Expected case names for older manifests')
a=p.parse_args()
rows=[json.loads(line) for line in (a.directory/'rows.jsonl').read_text().splitlines()]
manifest=json.loads((a.directory/'manifest.json').read_text())
assert all(r['valid'] for r in rows), 'Invalid requests retained in rows.jsonl; no performance conclusion'
lookup={(r['case'],r['contract'],r['request'],r['rep'],r['arm']):r for r in rows}
assert len(lookup)==len(rows), 'duplicate request identity'
geo=lambda xs: math.exp(statistics.mean(math.log(x) for x in xs))
expected_cases=manifest.get('cases') or a.case
assert expected_cases and set(expected_cases)=={r['case'] for r in rows}, 'missing or unexpected case'
expected_keys={(case,contract,request,rep,arm) for case in expected_cases for contract in manifest['contracts'] for request in ['cold','warm','edit'] for rep in range(manifest['reps']) for arm in manifest['arms'] if not (arm=='rust' and contract=='closed' and 'default' in manifest['contracts'])}
assert set(lookup)==expected_keys, 'missing or unexpected request'
cells=[]
for case in sorted(expected_cases):
 for contract in manifest['contracts']:
  for request in ['cold','warm','edit']:
   pairs=[]; times={arm:[] for arm in manifest['arms']}
   for rep in range(manifest['reps']):
    for arm in times:
     actual_contract='default' if arm=='rust' and 'default' in manifest['contracts'] else contract
     times[arm].append(lookup[case,actual_contract,request,rep,arm]['seconds']*1000)
    pairs.append(times['candidate'][-1]/times['baseline'][-1])
   cell=dict(case=case,contract=contract,request=request,repetitions=manifest['reps'],median_ms={k:statistics.median(v) for k,v in times.items()},candidate_baseline_geomean=geo(pairs),paired_range=[min(pairs),max(pairs)])
   if 'rust' in times:cell['candidate_rust_geomean']=geo([c/r for c,r in zip(times['candidate'],times['rust'])])
   cells.append(cell)
result={'requests':len(rows),'valid':len(rows),'cells':cells,'scope':f"small screen; {manifest['reps']} independent cache histories, not a population confidence interval or product gate",'by_contract':{}}
for contract in manifest['contracts']:
 result['by_contract'][contract]={}
 for request in ['cold','warm','edit']:
  selected=[c for c in cells if c['contract']==contract and c['request']==request]
  result['by_contract'][contract][request]={'candidate_baseline_geomean':geo([c['candidate_baseline_geomean'] for c in selected])}
(a.directory/'analysis.json').write_text(json.dumps(result,indent=2)+'\n')
for c in cells:print(c['case'],c['contract'],c['request'],{k:round(v,1) for k,v in c['median_ms'].items()},'new/old',round(c['candidate_baseline_geomean'],4),'new/rust',round(c.get('candidate_rust_geomean',0),4))
print(json.dumps(result['by_contract']))
