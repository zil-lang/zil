#!/usr/bin/env python3
"""Implementation-authored evidence audit, never a non-author source review."""
from __future__ import annotations
import argparse, collections, hashlib, json, math, random, statistics, tarfile
from pathlib import Path

def sha(p):
    with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def dump(p,x):p.write_text(json.dumps(x,ensure_ascii=False,indent=2)+'\n')
def main():
    ap=argparse.ArgumentParser();ap.add_argument('--evidence',type=Path,required=True);ap.add_argument('--out',type=Path,required=True)
    a=ap.parse_args();src=a.evidence;out=a.out;out.mkdir(parents=True,exist_ok=False)
    receipt=json.loads((src/'receipt.json').read_text())
    for name,v in receipt['files'].items():
        p=src/name;assert p.is_file() and p.stat().st_size==v['bytes'] and sha(p)==v['sha256'],name
    m=json.loads((src/'manifest.json').read_text())
    rows=[json.loads(x) for x in (src/'rows.jsonl').read_text().splitlines()]
    cells=collections.defaultdict(dict);ids=set();native=0;verified=0
    with tarfile.open(src/'raw.tar.gz') as tar:
        for name,h in m['source_hashes'].items():
            assert name.startswith('corpus/') and hashlib.sha256(tar.extractfile(name).read()).hexdigest()==h,name
        for r in rows:
            key=(r['case'],r['mode'],r['request'],r['block'],r['arm']);assert key not in ids;ids.add(key)
            assert r['warmup']==(r['block']<0)
            log='logs/'+Path(r['log']).name
            recorded=json.loads(tar.extractfile(log+'/record.json').read());assert recorded==r
            if not r['valid']:continue
            assert r['exit']==0 and not r['timeout']
            if r['arm']!='rust':
                report=json.loads(tar.extractfile(log+'/stdout').read())
                assert report['status']=='Success' and report['verification_runs']==1
                verified+=1
                assert report.get('native_codegen')==r.get('native_codegen')
                if r['mode']!='check':assert report['native_codegen']['backend']=='llvm-api'
            if r['mode']!='check':
                assert len(r['native_checks'])==3
                for check in r['native_checks']:
                    assert check['pass'] and check['exit']==0 and check['expected']==check['actual']
                    native+=1
            if not r['warmup']:cells[(r['case'],r['mode'],r['request'],r['arm'])][r['block']]=r
    cases=m['cases'];rounds=m['rounds'];profiles=[];deltas=[];severe=[]
    modes=['check','O0','O2'];requests=['cold','warm','private','edit-warm','public','restore']
    assert abs(sum(c['weight'] for c in cases)-1)<1e-10
    expected=sum((2 if mode=='check' or mode=='O0' else 3)*(rounds+1)*len(requests) for mode in modes)*len(cases)
    complete=len(rows)==expected and all(r['valid'] for r in rows)
    for c in cases:
        for mode in modes:
            for request in requests:
                pair=[cells[(c['id'],mode,request,arm)] for arm in ['base','cand']]
                if any(set(g)!=set(range(rounds)) for g in pair):complete=False;continue
                base,cand=[[g[b]['wall_ms'] for b in range(rounds)] for g in pair]
                mb,mc=statistics.mean(base),statistics.mean(cand)
                d={'case':c['id'],'mode':mode,'request':request,'base_mean_ms':mb,'candidate_mean_ms':mc,'saved_ms':mb-mc,'reduction':1-mc/mb,'paired_wins':sum(x<y for x,y in zip(cand,base)),'n':rounds}
                if mode=='O2':
                    rr=cells[(c['id'],mode,request,'rust')]
                    if set(rr)!=set(range(rounds)):complete=False
                    else:d['rust_mean_ms']=statistics.mean(r['wall_ms'] for r in rr.values())
                deltas.append(d)
                if mc>2*mb and mc-mb>1000:severe.append(d)
    if complete:
        for mode_name,mw in m['mode_weights'].items():
            for req_name,rw in m['request_weights'].items():
                assert abs(sum(mw.values())-1)<1e-9 and abs(sum(rw.values())-1)<1e-9
                mean={arm:0.0 for arm in ['base','cand']};excess={arm:{'100ms':0.0,'1000ms':0.0} for arm in mean};groups={}
                for c in cases:
                    for mode in modes:
                        for request in requests:
                            w=c['weight']*mw[mode]*rw[request]
                            for arm in mean:
                                values=[x['wall_ms'] for x in cells[(c['id'],mode,request,arm)].values()]
                                mean[arm]+=w*statistics.mean(values)
                                for threshold in [100,1000]:excess[arm][f'{threshold}ms']+=w*statistics.mean(max(x-threshold,0) for x in values)
                            v=w*(statistics.mean(x['wall_ms'] for x in cells[(c['id'],mode,request,'base')].values())-statistics.mean(x['wall_ms'] for x in cells[(c['id'],mode,request,'cand')].values()))
                            groups[c['group']]=groups.get(c['group'],0)+v
                rng=random.Random(20261109);ratios=[]
                for _ in range(2000):
                    sums={arm:0.0 for arm in mean}
                    for c in cases:
                        for mode in modes:
                            blocks=[rng.randrange(rounds) for _ in range(rounds)]
                            for request in requests:
                                w=c['weight']*mw[mode]*rw[request]
                                for arm in mean:sums[arm]+=w*statistics.mean(cells[(c['id'],mode,request,arm)][b]['wall_ms'] for b in blocks)
                    ratios.append(1-sums['cand']/sums['base'])
                ratios.sort()
                profiles.append({'profile':mode_name+'/'+req_name,'mean_ms':mean,'reduction':1-mean['cand']/mean['base'],'seconds_saved_per_1000':mean['base']-mean['cand'],'excess_wait_ms':excess,'group_saved_ms':groups,'paired_bootstrap95':[ratios[49],ratios[1949]],'interval_scope':'post-hoc fixed-method audit of this run; not a population or industrial guarantee'})
    eq=json.loads((src/'equivalence.json').read_text());assert all(e['text_equal'] for e in eq)
    rt=[json.loads(s) for s in (src/'runtime.jsonl').read_text().splitlines()] if (src/'runtime.jsonl').exists() else []
    quality=[]
    for c in cases:
        for snapshot in ['base','private']:
            med={}
            for arm in ['base','cand','rust']:
                vv=[r['wall_ms'] for r in rt if (r['case'],r['snapshot'],r['arm'])==(c['id'],snapshot,arm) and not r['warmup'] and r['valid']]
                if vv:med[arm]=statistics.median(vv)
            if med:quality.append({'case':c['id'],'snapshot':snapshot,'median_ms':med,'candidate_over_base':med.get('cand',float('inf'))/med.get('base',1)})
    upstream=json.loads((src/'upstream.json').read_text())
    originals=[{k:r.get(k) for k in ['crate','source_commit','status','rust_source_files','rust_source_bytes','zil_equivalent','timings']} for r in upstream['crates']]
    result={'status':'Pass for complete finite evidence' if complete else 'Incomplete; never renormalized','source':receipt['measured_source'],'baseline':receipt['baseline'],'receipt_sha256':sha(src/'receipt.json'),'compile_requests':len(rows),'expected':expected,'formal_requests':sum(not r['warmup'] for r in rows),'current_verify_runs':verified,'native_checks':native,'text_pairs':len(eq),'runtime_requests':len(rt),'profiles':profiles,'runtime':quality,'severe_tails':severe,'upstream_originals':originals,'originals_in_paired_weights':False,'non_author_review':'NotRun','merge_allowed':False}
    dump(out/'audit.json',result);dump(out/'deltas.json',deltas)
    text=['# 扩展覆盖与索引复用：实测复核','',f'基线 `{receipt["baseline"]}`；候选 `{receipt["measured_source"]}`。','',result['status']+'。所有频率是假设，不是遥测。原版Rust库没有ZIL等规模实现，不计入配对加权。','', '| 使用方式 | 基线ms | 候选ms | 节省% | 每1000次节省秒 |','|---|---:|---:|---:|---:|']
    for v in profiles:text.append(f'| {v["profile"]} | {v["mean_ms"]["base"]:.3f} | {v["mean_ms"]["cand"]:.3f} | {100*v["reduction"]:.2f} | {v["seconds_saved_per_1000"]:.3f} |')
    text+=['','## O2编辑与冷构建','| 样本 | 请求 | 基线ms | 候选ms | 节省ms | 配对胜出 |','|---|---|---:|---:|---:|---:|']
    for d in deltas:
        if d['mode']=='O2' and d['request'] in ['cold','private','warm']:text.append(f'| {d["case"]} | {d["request"]} | {d["base_mean_ms"]:.3f} | {d["candidate_mean_ms"]:.3f} | {d["saved_ms"]:.3f} | {d["paired_wins"]}/{d["n"]} |')
    text+=['','## 原版Rust库（非配对参考）','| 样本 | Rust文件数 | 源字节数 | 状态 |','|---|---:|---:|---|']
    for r in originals:text.append(f'| {r["crate"]} | {r["rust_source_files"]} | {r["rust_source_bytes"]} | {r["status"]} |')
    text+=['',f'编译请求{len(rows)}；原生输出核验{native}；原生代码段等同配对{len(eq)}；运行请求{len(rt)}。','', '审计由实施者编写，不冒充非作者源码复核；没有满足独审门，不自动合入。新索引复用与此前可选导入回退是不同候选，不混淆其基线。大规模真实应用的ZIL等价实现仍未完成。']
    (out/'RESULTS.md').write_text('\n'.join(text)+'\n')
    print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
