#!/usr/bin/env python3
"""Read-only check of published stage-3/4 table rows and same-source probe evidence."""
from pathlib import Path
import hashlib,json,subprocess
R=Path('/workspace/.zil-env/research');REPO=Path('/workspace/zil')
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
data=json.loads((R/'stage4-evidence-recomputed.json').read_text())
windows={x['window']:x for x in data['windows']}
wall={(w['window'],x['interface'],x['mode'],x['modules'],x['capacity'],x['request']):x for w in data['windows'] for x in w['all_cells']}
cpu={(w['window'],x['mode'],x['modules'],x['capacity'],x['request']):x for w in data['windows'] for x in w['all_cells'] if 'process_cpu_ms' in x}
mixed={(x['case'],x['opt']):x for x in data['mixed']['all_cells']}
p=REPO/'docs/research/incremental-compilation/LATEST-MEASUREMENTS.md';text=p.read_text();section='';seen_wall=set();seen_cpu=set();seen_mixed=set()
for line in text.splitlines():
 if line.startswith('## '):section=line[3:]
 if not line.startswith('| '):continue
 cells=[x.strip() for x in line.strip('|').split('|')]
 if cells[0] in ['cli/build','cli/check','session/build','session/check']:
  interface,mode=cells[0].split('/');n,capacity=map(int,cells[1].split('/'));key=(section,interface,mode,n,capacity,cells[2]);assert key not in seen_wall;seen_wall.add(key);x=wall[key]
  a=x['arms'];want=[f'{interface}/{mode}',f'{n}/{capacity}',key[-1],f'{x["baseline_ms"]:.3f}',f'{x["candidate_ms"]:.3f}',f'{x["candidate_over_baseline"]:.4f}',f'{a["baseline"]["median_rss_kib"]:.0f}/{a["candidate"]["median_rss_kib"]:.0f}',f'{a["rust"]["median_block_mean_ms"]:.3f}' if 'rust' in a else '—'];assert cells==want,(key,cells,want)
 elif cells[0] in ['stage4-main-final','stage4-isolation']:
  n,capacity=map(int,cells[2].split('/'));key=(cells[0],cells[1],n,capacity,cells[3]);assert key not in seen_cpu;seen_cpu.add(key);x=cpu[key]
  want=[key[0],key[1],f'{n}/{capacity}',key[-1],f'{x["process_cpu_ms"]["baseline"]:.3f}',f'{x["process_cpu_ms"]["candidate"]:.3f}',f'{x["candidate_over_baseline_process_cpu"]:.4f}'];assert cells==want,(key,cells,want)
 elif cells[0].endswith('.zil'):
  key=(cells[0],cells[1]);assert key not in seen_mixed;seen_mixed.add(key);x=mixed[key];t=x['median_block_mean_ms']
  want=[*key,f'{t["baseline"]:.3f}',f'{t["candidate"]:.3f}',f'{x["candidate_over_baseline"]:.4f}' if x['semantically_equivalent'] else '无效（误编译）',str(x['baseline_wrong_results'])];assert cells==want,(key,cells,want)
assert seen_wall==set(wall) and len(seen_wall)==176
assert seen_cpu==set(cpu) and len(seen_cpu)==48
assert seen_mixed==set(mixed) and len(seen_mixed)==16
for w in data['windows']:
 assert f'{w["blocks"]} blocks，{w["rows"]} 请求。墙钟不利单元 {len(w["time_unfavorable_cells"])}/44。' in text
p2=R/'stage4-probe-source-binding.json';binding=json.loads(p2.read_text());checked={str(p):sha(p),str(p2):sha(p2)}
for name,digest in binding['sources'].items():
 assert sha(REPO/name)==digest==hashlib.sha256(subprocess.check_output(['git','show',binding['source_commit']+':'+name],cwd=REPO)).hexdigest();checked[str(REPO/name)]=digest
for b in binding['builds']:
 original=Path(b['original_path']);replica=Path(b['command'][-1]);assert sha(original)==sha(replica)==b['original_sha256']==b['reproduced_sha256'] and b['identical'];checked[str(original)]=sha(original);checked[str(replica)]=sha(replica)
 assert all(i['recorded_sha256']==b['original_sha256'] for w in data['windows'] for i in w['identities'] if i['path']==str(original))
current=REPO/'docs/research/incremental-compilation/CURRENT.md';current_text=current.read_text();main=windows['stage4-main-final']
keys=[('session','check',512,32,'unchanged'),('session','check',512,32,'private'),('session','check',128,32,'unchanged'),('session','build',128,32,'unchanged'),('session','build',128,32,'private'),('cli','build',128,0,'cold'),('cli','build',32,0,'private1')]
main_rows=[line for line in current_text.splitlines() if line.startswith('| 512模块') or line.startswith('| 128模块') or line.startswith('| 32模块')]
assert len(main_rows)==len(keys)==7
for line,key in zip(main_rows,keys):
 cells=[x.strip() for x in line.strip('|').split('|')];x=wall[('stage4-main-final',*key)];assert cells[1:3]==[f'{x["baseline_ms"]:.3f}',f'{x["candidate_ms"]:.3f}']
 if '倍' in cells[3]:assert f'{1/x["candidate_over_baseline"]:.2f}倍' in cells[3]
 if '降低' in cells[3]:assert f'降低{100*(1-x["candidate_over_baseline"]):.1f}%' in cells[3]
 if '增加' in cells[3]:assert f'增加{100*(x["candidate_over_baseline"]-1):.1f}%' in cells[3]
for key in [('session','check',512,32,'private'),('session','check',512,32,'unchanged'),('session','check',128,8192,'private')]:
 x=wall[('stage4-main-final',*key)]['arms'];assert f'{x["baseline"]["median_rss_kib"]:,.0f}→{x["candidate"]["median_rss_kib"]:,.0f} KiB' in current_text
for request in ['unchanged','private']:
 x=cpu[('stage4-main-final','check',512,32,request)]['process_cpu_ms'];assert f'{x["baseline"]:.3f}→{x["candidate"]:.3f} ms' in current_text
from fractions import Fraction
def median(v):
 v=sorted(v);n=len(v);return (v[(n-1)//2]+v[n//2])/2
rs=[json.loads(line) for line in (R/'stage4-main-final/rows.jsonl').read_text().splitlines()]
selected=[x for x in rs if x['interface']=='session' and x['modules']==512 and x['capacity']==32 and x['mode']=='check' and x['arm']=='candidate' and x['request']=='unchanged'];assert len(selected)==20
raw=median([Fraction(x['phases']['verify']['interval_ns']) for x in selected])/1000000
means=[sum(Fraction(x['phases']['verify']['interval_ns']) for x in selected if x['block']==b)/2 for b in range(10)];mid=median(means)/1000000
assert f'{float(raw):.3f}' in current_text and f'{float(mid):.3f}' in current_text and '20次请求' in current_text
checked[str(current)]=sha(current)
for name in ['PLAN.md','PATH-RESOLUTION.md','FRONTEND-REUSE.md']:
 f=REPO/'docs/research/incremental-compilation'/name;checked[str(f)]=sha(f)
for name,expected in [('RESULTS.md','53730fdd521e1fd8e2e9b0312e834926ed6e5b668c32856d6715ecd04c05e69e'),('MEASUREMENTS.md','58f24cc5561bb76acc06ef0c04ca0b8a18adbfcf487827431826a23c4e866af7')]:
 f=REPO/'docs/research/incremental-compilation'/name;assert sha(f)==expected;checked[str(f)]=expected
result={'reviewer':'/root/review_incremental_slice1','verdict':'Pass','scope':'Exact latest table rows and source/reproduced probe identity only; no benchmark execution','current_main_rows':7,'current_rss_groups':3,'current_cpu_groups':2,'verify_event_raw_median_ms':float(raw),'verify_event_median_block_mean_ms':float(mid),'wall_rows':176,'mixed_rows':16,'process_cpu_rows':48,'all_table_fields_exact_at_printed_precision':True,'all_three_reproduced_probe_elf_hashes_match_originals_and_manifests':True,'sources_match_exact_commit':binding['source_commit'],'hashes':checked}
(R/'stage4-document-review.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='hashes'},ensure_ascii=False))
