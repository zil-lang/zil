#!/usr/bin/env python3
"""Block-paired inference. Six blocks = exploration, not formal release gate."""
import json,pathlib,statistics,random,math,sys
root=pathlib.Path(sys.argv[1]);rows=[json.loads(x)for x in(root/'timings158.jsonl').read_text().splitlines()];spec=json.loads((root/'frozen158.json').read_text());assert all(r['pass']for r in rows)
weights={c['name']:c['weight']for c in spec['cases']};cats={c['name']:c['category']for c in spec['cases']};sw=spec['scenario_weights'];blocks=range(spec['blocks']);cases=list(dict.fromkeys(r['case']for r in rows));cells={}
for b in blocks:
 for c in cases:
  for v in ['A','C']:
   rs=[r for r in rows if(r['block'],r['case'],r['variant'])==(b,c,v)];assert len(rs)==2
   cells[b,c,v]={'request':statistics.mean(sum(sw[x['scenario']]*x['api_wall_ns']for x in r['requests'])for r in rs),'process':statistics.mean(r['process_ns']for r in rs),'rss':statistics.mean(r['vmhwm_kib']for r in rs)}
def ratio(xs,metric,w):
 pairs=[(sum(w[c]*cells[b,c,'A'][metric]for c in xs),sum(w[c]*cells[b,c,'C'][metric]for c in xs))for b in blocks];a=sum(x[0]for x in pairs);c=sum(x[1]for x in pairs);rnd=random.Random(158);boot=[]
 for _ in range(8000):
  seq=[pairs[rnd.randrange(len(pairs))]for j in pairs];boot.append(sum(z[1]for z in seq)/sum(z[0]for z in seq))
 boot.sort();norm=sum(w[c]for c in xs)*len(pairs)
 return {'ratio_C_over_A':c/a,'baseline_mean':a/norm,'candidate_mean':c/norm,'exploratory_paired_bootstrap95':[boot[199],boot[7799]],'blocks':len(pairs),'block_ratios':[y/x for x,y in pairs]}
out={'schema':'zil158-results/1','processes':len(rows),'requests':sum(len(x['requests'])for x in rows),'common_profile_coverage':sum(weights[c]for c in cases),'groups':{},'per_case':{},'limits':'six blocks on shared host; bootstrap descriptive, no multiplicity-adjusted or formal release claim; weighted workload assumption not production telemetry'}
for g in ['weighted','large','consumer','medium','tiny','equal']:
 xs=[c for c in cases if g in ['weighted','equal']or cats[c]==g];w={c:1 if g=='equal'else weights[c]for c in xs};out['groups'][g]={m:ratio(xs,m,w)for m in ['request','process','rss']}
for c in cases:out['per_case'][c]={m:ratio([c],m,{c:1})for m in ['request','process','rss']}
(root/'summary158.json').write_text(json.dumps(out,indent=2));print(json.dumps(out['groups'],indent=2))
