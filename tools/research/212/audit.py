#!/usr/bin/env python3
"""Second arithmetic path over raw rows. Author recomputation, NOT independent review."""
import argparse, hashlib, json, math, random, statistics
from collections import defaultdict
from pathlib import Path


def audit(root):
    plan=json.loads((root/'plan.json').read_text())
    result=json.loads((root/'summary.json').read_text())
    rows=[json.loads(s) for s in (root/'measure/records.jsonl').read_text().splitlines()]
    order=[(x['block'],x['case'],x['position'],x['arm'],r) for x in plan['order'] for r in plan['request_weights']]
    assert [(x['block'],x['case'],x['position'],x['arm'],x['request']) for x in rows]==order
    assert all(x['pass'] and x['exit']==0 and not x['timeout'] for x in rows)
    assert all(x.get('verification_runs')==1 for x in rows if x['language']=='zil')
    assert all(all(c['matches'] for c in x['checks']) for x in rows)
    blocks=list(range(plan['blocks'])); wall_cells=None
    for metric in ['wall','cpu']:
        grouped=defaultdict(list)
        for x in rows:
            grouped[x['case'],x['request'],x['arm'],x['block']].append(x['wall_seconds'] if metric=='wall' else x['cpu_user']+x['cpu_system'])
        cells={k:statistics.fmean(v) for k,v in grouped.items()}
        if metric=='wall':wall_cells=cells
        totals=dict(A=0.,B=0.)
        by_request={r:dict(A=0.,B=0.) for r in plan['request_weights']}
        for case in plan['cases']:
            totals_case=dict(A=0.,B=0.)
            for r,w in plan['request_weights'].items():
                for arm in 'AB':
                    t=statistics.median([cells[case['id'],r,arm,b] for b in blocks])
                    totals_case[arm]+=w*t;by_request[r][arm]+=case['weight']*t
                    totals[arm]+=case['weight']*w*t
            for arm in 'AB':assert math.isclose(totals_case[arm],result[metric]['by_case'][case['id']][arm],rel_tol=1e-12)
        for r,v in by_request.items():
            for arm in 'AB':assert math.isclose(v[arm],result[metric]['by_request'][r][arm],rel_tol=1e-12)
        for arm in 'AB':assert math.isclose(totals[arm],result[metric][arm],rel_tol=1e-12)
        assert math.isclose(totals['B']/totals['A'],result[metric]['B_over_A'],rel_tol=1e-12)
    rng=random.Random(212);boot=[]
    for _ in range(5000):
        sample=rng.choices(blocks,k=len(blocks));totals=dict(A=0.,B=0.)
        for c in plan['cases']:
            for r,w in plan['request_weights'].items():
                for arm in 'AB':totals[arm]+=c['weight']*w*statistics.median(wall_cells[c['id'],r,arm,b] for b in sample)
        boot.append(totals['B']/totals['A'])
    boot.sort()
    assert all(math.isclose(a,b,rel_tol=1e-12) for a,b in zip([boot[124],boot[4874]],result['exploratory_paired_block_bootstrap95']))
    cache_entries=0
    for work in (root/'measure').iterdir():
        if not work.is_dir() or work.name=='logs':continue
        for p in (work/'objects').glob('objects-v1/*/*.zobj'):
            data=p.read_bytes();assert data[:8]==b'ZILOBJ01' and len(data)>=80
            assert data[8:40].hex()==p.stem
            assert int.from_bytes(data[72:80],'little')==len(data)-80
            assert hashlib.sha256(data[80:]).digest()==data[40:72]
            cache_entries+=1
    return {'plan':root.name,'status':'Pass','rows':len(rows),'native_checks':sum(len(x['checks']) for x in rows),
            'cache_entries_checked':cache_entries,'arithmetic':'wall/cpu totals + groups + requests + paired bootstrap',
            'reviewer':'same author, alternate arithmetic implementation', 'degenerate_single_block_interval':len(blocks)==1}

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('root',type=Path);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
    r=[audit(d) for d in sorted(a.root.glob('perf-*')) if (d/'summary.json').exists()]
    a.out.write_text(json.dumps({'status':'Pass','runs':r},indent=2)+'\n');print(json.dumps(r))
