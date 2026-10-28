#!/usr/bin/env python3
"""Post-hoc audit of frozen 211 one-CGU measurements; never rewrites inputs.
The original cross-path exact-text assertion is retained as failed. Only the
verified-fingerprint comment is excluded in this separate, explicit audit copy.
"""
import argparse, hashlib, json, statistics
from pathlib import Path

def sha(data): return hashlib.sha256(data).hexdigest()
def normalized(data):
    lines=data.splitlines(keepends=True)
    matches=[i for i,line in enumerate(lines) if line.startswith(b'; verified-fingerprint: ')]
    if len(matches)!=1: raise ValueError('expected exactly one standalone fingerprint comment')
    del lines[matches[0]]
    return b''.join(lines)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--primary-plan',type=Path,required=True)
    p.add_argument('--directory',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True)
    a=p.parse_args(); mainplan=json.loads(a.primary_plan.read_text()); plan=json.loads((a.directory/'plan.json').read_text())
    assert plan['primary_plan_sha256']==sha(a.primary_plan.read_bytes())
    rows=[json.loads(x) for x in (a.directory/'records.jsonl').read_text().splitlines()]
    assert len(rows)==plan['builds']==len(plan['order'])
    for index,(row,spec) in enumerate(zip(rows,plan['order'])):
        assert all(row[k]==spec[k] for k in ['case','profile','arm']) and row['pass']
        ir=(a.directory/f'work-{index}'/'program.ll').read_bytes()
        assert sha(ir)==row['llvm_sha256']
        row['audit_normalized_llvm_sha256']=sha(normalized(ir))
    result={'status':'PassPostHocCommentAudit','initial_exact_text_assertion':'Fail; retained in original log',
            'analysis_scope':'Explicit post-hoc auxiliary audit; not frozen primary criterion; no generated input changed',
            'plan_sha256':sha((a.directory/'plan.json').read_bytes()),'records_sha256':sha((a.directory/'records.jsonl').read_bytes()),
            'builds':len(rows),'successful_builds':sum(r['pass'] for r in rows),'profiles':{},'artifact_groups':[]}
    for profile in ['O0','O2']:
        out={'A':0.,'B':0.,'projects':{}}
        for case in mainplan['cases']:
            rs=[r for r in rows if r['case']==case['id'] and r['profile']==profile];assert len(rs)==4
            assert len({r['executable_sha256'] for r in rs})==1
            assert len({r['audit_normalized_llvm_sha256'] for r in rs})==1
            result['artifact_groups'].append({'case':case['id'],'profile':profile,'executable_sha256':rs[0]['executable_sha256'],
                'audit_normalized_llvm_sha256':rs[0]['audit_normalized_llvm_sha256'],'raw_llvm_distinct_hashes':len({r['llvm_sha256'] for r in rs})})
            group={arm:statistics.mean(r['wall_seconds'] for r in rs if r['arm']==arm) for arm in 'AB'}
            group['B_over_A']=group['B']/group['A'];out['projects'][case['id']]=group
            for arm in 'AB':out[arm]+=case['weight']*group[arm]
        out['B_over_A']=out['B']/out['A'];result['profiles'][profile]=out
    result['scope']='One ABBA block per project/profile; fresh compiler cache; only cold builds. Descriptive, no CI, not full-request weighted result.'
    a.out.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
if __name__=='__main__':main()
