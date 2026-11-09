#!/usr/bin/env python3
"""Mechanical audit of a frozen experiment; not independent source review."""
import collections
import hashlib
import json
import math
from pathlib import Path
import statistics
import tarfile

HERE = Path(__file__).resolve().parent
RUN = '42612992774'
DIR = HERE / 'native' / RUN
REQUESTS = ('cold', 'warm', 'private', 'edit-warm', 'public', 'restore')
ARMS = ('base', 'cand', 'rust')
CASES = ('tree32x8', 'tree256x8')


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def med(rows, key):
    return statistics.median(row[key] for row in rows)


def main():
    receipt = json.loads((DIR/'receipt.json').read_text())
    for name, identity in receipt['files'].items():
        path = DIR/name
        assert path.stat().st_size == identity['bytes'], name
        assert sha(path) == identity['sha256'], name
    rows = [json.loads(line) for line in (DIR/'rows.jsonl').read_text().splitlines()]
    keys = {(r['case'], r['block'], r['request'], r['arm']) for r in rows}
    expected = {(c, b, q, a) for c in CASES for b in range(-1,7) for q in REQUESTS for a in ARMS}
    assert len(rows) == len(keys) == len(expected) == 288 and keys == expected
    native_checks = verifies = 0
    with tarfile.open(DIR/'raw.tar.gz', 'r:gz') as archive:
        for row in rows:
            assert row['valid'] and row['exit'] == 0 and not row['timeout']
            assert row['warmup'] == (row['block'] == -1)
            assert row['wall_ms'] > 0 and row['cpu_ms'] >= 0 and row['rss_kib'] > 0
            assert [c['size'] for c in row['native_checks']] == [0,7,256]
            assert all(c['pass'] and c['exit'] == 0 and not c['stderr_hex']
                       and c['stdout_sha256'] == c['expected_sha256'] for c in row['native_checks'])
            native_checks += len(row['native_checks'])
            assert json.load(archive.extractfile(row['log']+'/record.json')) == row
            if row['arm'] != 'rust':
                stdout = json.load(archive.extractfile(row['log']+'/stdout'))
                assert stdout['status'] == 'Success' and stdout['verification_runs'] == 1
                assert stdout['native_codegen'] == row['native_codegen']
                assert row['native_codegen']['backend'] == 'llvm-api'
                assert row['native_codegen']['actual_units'] == (1 if row['arm']=='base' else 2)
                if row['request'] in ('warm', 'edit-warm'):
                    cache = row['native_codegen']['object_cache']
                    assert cache['hits'] == row['native_codegen']['actual_units'] and cache['misses'] == 0
                    assert all(o['input_kind']=='object-cache' and not o['llvm_verification_this_request']
                               for o in row['native_codegen']['object_reports'])
                verifies += 1
        native_log = archive.extractfile('setup/native-controls.log').read().decode()
    cells = collections.defaultdict(dict)
    for row in rows:
        if not row['warmup']:
            cells[(row['case'],row['request'],row['arm'])][row['block']] = row
    comparisons=[]
    for case in CASES:
        for request in REQUESTS:
            groups = {arm: [cells[(case,request,arm)][b] for b in range(7)] for arm in ARMS}
            ratios=[groups['cand'][b]['wall_ms']/groups['base'][b]['wall_ms'] for b in range(7)]
            comparisons.append({'case':case,'request':request,'n':7,
                'median_ms':{a:med(v,'wall_ms') for a,v in groups.items()},
                'median_cpu_ms':{a:med(v,'cpu_ms') for a,v in groups.items()},
                'median_rss_kib':{a:med(v,'rss_kib') for a,v in groups.items()},
                'median_executable_bytes':{a:med(v,'executable_bytes') for a,v in groups.items()},
                'candidate_base_ratio_of_medians':med(groups['cand'],'wall_ms')/med(groups['base'],'wall_ms'),
                'paired_geometric_ratio':math.exp(statistics.mean(math.log(x) for x in ratios)),
                'paired_candidate_wins':sum(x<1 for x in ratios),'paired_ratios':ratios,
                'max_ms':{a:max(r['wall_ms'] for r in v) for a,v in groups.items()},
                'object_hits_misses':{a:sorted({(r['native_codegen']['object_cache']['hits'],r['native_codegen']['object_cache']['misses']) for r in groups[a]}) for a in ('base','cand')}})
    audit={'status':'Pass: mechanical evidence consistency, not performance acceptance',
           'source_commit':receipt['source_commit'],'source_sha256':receipt['source_sha256'],
           'run':receipt['run'],'compile_requests':len(rows),'formal_requests':sum(not r['warmup'] for r in rows),
           'native_checks':native_checks,'current_zil_verifications':verifies,'comparisons':comparisons,
           'decision':'Hold / NoGo for production adoption of this candidate',
           'reason':'Large-case cold and warm regressions, increased CPU/RSS and executable size; edit improvement alone is insufficient.',
           'independent_review':'NotRun','runtime_throughput':'NotRun','full_language_matrix':'NotRun',
           'merge_allowed':False,'audit_sha256':sha(__file__),'native_unit_test_log_tail':native_log[-2500:]}
    (HERE/'audit-result.json').write_text(json.dumps(audit,indent=2)+'\n')
    lines=['# 编译专项首轮：有界缩并的真实收益与否决原因','',
        '**裁决：首个候选 Hold / 不合入主线。专项尚未完成。**','',
        '固定基线 `327eccfaf3169d1404534ddc16359310a21a40ad`；受测候选 `'+receipt['source_commit']+'`。',
        '实际实现将完整调用闭包复制成本超限的直接单CGU回退，改为8→4→2有界缩并。没有放宽Verify、机器预检、复制预算或LLVM优化。','',
        '## 同机完整CLI对照','',
        'Linux GitHub runner，同一官方LLVM23.1.4安装、Rust1.99.1；两个可用逻辑CPU，ZIL8CGU/2jobs、incremental-o2/after、显式语义与对象缓存，Rust保留默认增量分区。空项目缓存冷请求，OS文件缓存不清；计时包含新进程到链接完成，不含工具安装、编译器构建或程序运行。',
        '两种动态输入树；每种一个预热和七个独立缓存历史，逐请求交错三臂。下表单位ms；百分比是两列中位数之比，不冒充置信区间。','',
        '| 模块数 | 请求 | 原ZIL | 候选ZIL | Rust | 候选耗时变化 | 配对胜出 |',
        '| --- | --- | ---: | ---: | ---: | ---: | ---: |']
    names={'cold':'冷编译','warm':'无修改重编译','private':'函数体修改','edit-warm':'编辑后暖编译','public':'签名与调用修改','restore':'恢复原源码'}
    for c in comparisons:
        v=c['median_ms']; delta=100*(c['candidate_base_ratio_of_medians']-1)
        lines.append(f"| {c['case']} | {names[c['request']]} | {v['base']:.1f} | {v['cand']:.1f} | {v['rust']:.1f} | {delta:+.1f}% | {c['paired_candidate_wins']}/7 |")
    lines += ['', '## 机制与代价','',
        '原版所有受测请求为1CGU，候选为2CGU。无修改和编辑后暖请求均全部对象命中；暖退化不能归因于缓存没命中。命中后仍有完整当前Verify、机器模型处理、完整对象键、机器预检和链接成本。实际命中数、每对象时间及逐轮数据见rows.jsonl；不同工作线程时间可能重叠，不能相加冒充完整墙钟。',
        '256模块冷编译CPU中位数由约1.75秒增至3.52秒，暖编译进程RSS也增大。可执行文件大小已记录，尚未测运行吞吐，不能声称运行性能不变。',
        '这证明满足原复制上限并保留两个分区不足以证明编译净收益。完整闭包重复优化仍让大单元留在关键路径；下一候选必须同时约束最大单元成本、总复制成本、编辑失效范围和缓存命中后处理成本。','',
        '## 核验范围','',
        f'本轮{len(rows)}次完整编译（{sum(not r["warmup"] for r in rows)}次正式计时）、{native_checks}次原生输出检查通过；其中ZIL每次执行当前独立Verify，共{verifies}次。每个产物检查0/7/256字节输入，256字节序列覆盖所有字节值。',
        '另有21项参考分区/布局Rust控制、Clippy/格式和20,996项有限图模型检查通过；LLVM feature分区控制另由本轮CI执行。模型只验证有限图闭包/预算，不替代语言安全或原生证明。',
        '机械审计核对保存文件摘要/字节数、288条唯一矩阵记录、原始日志与JSON逐条一致、实际后端、Verify计数及暖命中；该审计由实施者编写，不冒称非作者独审。',
        '未完成：独立源码审核、生成程序运行吞吐/内存/代码质量门、泛型/CT/资源密集矩阵、工业项目及更大规模、旧Stage15前端退化逐提交归因。','',
        '## 继续实施方向','',
        '1. 调用图/强连通分量感知的稳定分区，并计量完整闭包复制的关键路径；若引入选择性导入，单独立合同并保护原生运行质量，不能默默截断旧闭包合同。',
        '2. 对已记录的对象key/preflight和布局处理成本做因果拆分；ZIL已有结构化缓存键，不重复添加一层相同缓存。',
        '3. 按声明/类型布局/函数体/CT值/效果等真实依赖拆前端查询；结果不变时停止传播，不能删掉必要依赖或复用旧Verify授权。先补低扰动阶段观测，再追旧Stage15→主线的具体回退。',
        '近期一手资料及与ZIL的适用边界见RESEARCH.md。','',
        '## 原始证据与复现','',
        f'- 本次完整运行：{receipt["run"]}',
        f'- 精确运行文件：native/{RUN}/manifest.json、rows.jsonl、summary.json、raw.tar.gz、receipt.json。',
        '- 审计：audit.py；结果audit-result.json。测量器：measure.py。CI配方：.github/workflows/compiler-cost-native.yml。',
        '- 基线和候选编译器分别release构建、独立target目录。首轮reference CI缺少离线依赖的失败仍保留；修正仅增加cargo fetch --locked准备，未改锁文件或将失败重标成功。',
        '- 不将这次新工具链/新CPU的绝对毫秒数与旧报告直接相减，不以两份语料关闭产品父门。','']
    (HERE/'RESULTS.md').write_text('\n'.join(lines))
    print(json.dumps(audit,indent=2))


if __name__=='__main__':
    main()
