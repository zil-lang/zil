#!/usr/bin/env python3
from pathlib import Path
import hashlib,json
REPO=Path('/workspace/zil');ROOT=Path('/workspace/.zil-env/research');OUT=ROOT/'stage5-followup'
docs=REPO/'docs/research/incremental-compilation'
text=(docs/'DISTRIBUTION-RESULTS.md').read_text();assert 'FINAL_FOLLOWUP_RESULTS' not in text
original=json.loads((ROOT/'stage5/data-review-recomputed.json').read_text())['windows']
final=json.loads((OUT/'data-review-recomputed.json').read_text())['windows']
checked=[]
def cell(w,mode,request,case='large-8192'):
    return next(c for c in w['cells'] if (c['case'],c['mode'],c['request'])==(case,mode,request))
labels={'冷构建':'cold','无改动':'unchanged','前部私有修改':'early_edit','末尾私有修改':'late_edit'}
for block,data,prefix,expected_rows in [(text.split('## 广分布结果')[1].split('## 最终机器预检优化')[0],original,'generated-',9),(text.split('## 最终机器预检优化')[1].split('## 已落地的四个切口')[0],final,'followup-',10)]:
    lines=[line for line in block.splitlines() if line.startswith('| 默认') or line.startswith('| 显式封闭')]
    assert len(lines)==expected_rows
    for line in lines:
        label,z,r,ratio=[x.strip() for x in line.strip('|').split('|')]
        w=data[prefix+('closed' if label.startswith('显式封闭') else 'default')]
        mode='O2' if 'O2' in label else 'O0';request=next(v for k,v in labels.items() if label.endswith(k))
        c=cell(w,mode,request)
        assert [z,r,ratio]==[f'{c["arms"]["zil"]["median_ms"]:.3f}',f'{c["arms"]["rust"]["median_ms"]:.3f}',f'{c["ratio"]:.3f}']
        checked.append({'window':prefix+('closed' if label.startswith('显式封闭') else 'default'),'mode':mode,'request':request,'table':line})
iso=final['followup-isolation']
for case,mode,request in [('large-8192','O2','unchanged'),('large-8192','O0','late_edit'),('large-8192','O0','early_edit'),('large-8192','O2','early_edit'),('large-512','O2','unchanged'),('body-16x16','O0','unchanged')]:
    c=cell(iso,mode,request,case)
    pair=f'{c["arms"]["baseline"]["median_ms"]:.3f}→{c["arms"]["zil"]["median_ms"]:.3f}'
    assert pair in text,pair
    checked.append({'window':'followup-isolation','case':case,'mode':mode,'request':request,'narrative_pair':pair})
for mode,request in [('O2','unchanged'),('O0','late_edit'),('O0','early_edit')]:
    c=cell(iso,mode,request);interval=f'[{c["exploratory_95"][0]:.3f},{c["exploratory_95"][1]:.3f}]';assert interval in text
c=cell(iso,'O2','unchanged','large-512');assert c['exploratory_95'][0]>1
assert f'[{c["exploratory_95"][0]:.3f},{c["exploratory_95"][1]:.3f}]' in text
c=cell(iso,'O2','unchanged');assert all(x==1 for x in c['arms']['zil']['hits']) and all(x==0 for x in c['arms']['zil']['misses'])
assert f'{c["arms"]["baseline"]["backend_component_sum_ms"]["preflight_ns"]:.3f}→{c["arms"]["zil"]["backend_component_sum_ms"]["preflight_ns"]:.3f}' in text
assert f'{c["arms"]["baseline"]["median_rss_mib"]:.2f}→{c["arms"]["zil"]["median_rss_mib"]:.2f}' in text
for word in ['10524','877','8628','22740','6042','program.ll早于分区/内部化','ZIL JSON收据输出','输出文件集合不同','Rust内置LLVM23.1.1','宿主噪声/温度未严格控制','不能据此宣布整个语言','main未合入','未建立产品四目标合取']:
    assert word in text,word
assert (docs/'DISTRIBUTION-TABLES.md').read_bytes()==(ROOT/'stage5/analysis/DISTRIBUTION-TABLES.md').read_bytes()
assert (docs/'PREFLIGHT-FOLLOWUP-TABLES.md').read_bytes()==(OUT/'analysis/DISTRIBUTION-TABLES.md').read_bytes()
paths=[docs/x for x in ['DISTRIBUTION-RESULTS.md','DISTRIBUTION-TABLES.md','PREFLIGHT-FOLLOWUP-TABLES.md','ALGORITHM-ATTRIBUTION.md','PLAN.md','README.md','DISTRIBUTION-PROTOCOL.md','PREFLIGHT-FOLLOWUP-PROTOCOL.md']]
paths += [REPO/f'evidence/research/incremental-compilation/{x}/README.md' for x in ['stage5','stage5-followup']]
paths += sorted((docs/'plots').rglob('*.svg'))
paths += [REPO/'tools/research/incremental-compilation/distribution_plot.py',REPO/'tools/research/incremental-compilation/distribution_report.py']
result={'pass':True,'checks':checked,'scope':'All original19 key Rust table rows,6 isolation narrative pairs,main interval/preflight/RSS anchors and final boundaries; full877 tables separately independently recomputed. Historical algorithm attribution was read against original sources, current59d mechanism against exact review.','documents':{str(p.relative_to(REPO)):{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in paths}}
(OUT/'document-review.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps({'pass':True,'numeric_anchors':len(checked),'documents':len(paths)}))
