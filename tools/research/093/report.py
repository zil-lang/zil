#!/usr/bin/env python3
"""Publish compact receipts and every preregistered performance cell."""
import argparse, json, shutil, statistics
from pathlib import Path
from run import BASE, sha, write

def main():
    p=argparse.ArgumentParser();p.add_argument('--runs',type=Path,required=True);p.add_argument('--repo',type=Path,required=True)
    a=p.parse_args();runs=a.runs.resolve();repo=a.repo.resolve();f=runs/'frozen-003'
    summary=json.loads((f/'summary.json').read_text());identity=json.loads((f/'identity.json').read_text())
    evidence=repo/'evidence/impl/093';evidence.mkdir(parents=True,exist_ok=True)
    for source,name in [(f/'identity.json','h-identity.json'),(f/'functional.json','h-functional.json'),
        (f/'counts.json','h-counts.json'),(f/'summary.json','h-summary.json'),
        (f/'sample-start.json','h-sample-start.json'),(f/'sample-finish.json','h-sample-finish.json'),
        (runs/'lab-replay-001/result.json','lab-replay.json'),
        (runs/'structure-003/result.json','structure.json'),
        (runs/'design-feedback-001/result.json','design-feedback.json'),
        (runs/'recompute-001/result.json','recompute.json')]:shutil.copy2(source,evidence/name)
    allrows=summary['cells']+summary['processes']
    production=[r for r in allrows if r['case'] in ['owned-main','slices-main']]
    gate=[r for r in allrows if r.get('scenario','process') in ['cold','process'] or r['mode']=='build']
    def census(rows):return dict(cells=len(rows),interval_below_one=sum(r['exploratory_95'][1]<1 for r in rows),
        interval_above_one=sum(r['exploratory_95'][0]>1 for r in rows),
        interval_overlaps_one=sum(r['exploratory_95'][0]<=1<=r['exploratory_95'][1] for r in rows))
    assert summary['production_admission']=='NoGo'
    decision=dict(status='Pass',research_slice='093',schema=16,baseline=BASE,
        preregistration_commit='19949d4e',candidate=identity['variants']['H'],
        formal_window=json.loads((f/'sample-start.json').read_text()),
        h_production_admission='NoGo',all_cell=census(allrows),production_cell=census(production),
        preregistered_gate_cells=census(gate),regressions=summary['regressions'],
        full_process_stable_improvements=sum(r['exploratory_95'][1]<1 for r in summary['processes']),
        prior_lab_test_functions=147,prior_lab_programs=511,prior_lab_queries=12168,
        production_bridge_cases=18,production_bridge_instrumented_control_cases=18,project_bridge_requests=16,
        structural_model_systems=2304,structural_model_local_safety_queries=36864,
        base_unit_tests=8,sha_related_unit_tests_in_base_suite=3,driver_all_raw_variant_test=1,
        h_functional_requests=128,h_guard_requests=284,h_native_executions=16,
        formal_processes=128,formal_requests=512,
        limitations=summary['limitations'],
        production_rollback='production-rollback.json',raw_archive='See raw-archive.json, generated after final raw snapshot',
        scope='Research result and failed candidate rejection; no production safety kernel replacement, no Rust performance superiority')
    write(evidence/'result.json',decision)
    lines=['# 093 全部正式成本单元','',
        'B = schema16 冻结生产基线；H = 相同 Debug 字节、4096 字节缓冲的流式 SHA。',
        '每单元 1 对预热与 7 对保留；以下仅统计 7 对保留。128 进程 / 512 请求全数保留。',
        'H/B 小于 1 表示候选较快。比率是逐对比率的中位数，不是两列中位耗时相除。',
        '区间为预登记的 2000 次配对中位 bootstrap 探索性区间，未校正多重比较。','',
        '## 完整进程','',
        '| 场景 | 阶段 | B 中位 ms | H 中位 ms | 配对 H/B | 探索性 95% | B/H 父进程 VmHWM 中位 KiB |',
        '|---|---|---:|---:|---:|---|---|']
    for r in summary['processes']:
        lo,hi=r['exploratory_95'];mem=r['vmhwm_kib']
        lines.append(f"| {r['case']} | {r['mode']} | {r['medians']['B']/1e6:.3f} | {r['medians']['H']/1e6:.3f} | {r['paired_ratio']:.6f} | [{lo:.6f}, {hi:.6f}] | {statistics.median(mem['B']):g} / {statistics.median(mem['H']):g} |")
    lines+=['','VmHWM 仅覆盖编译父进程，不含 clang 子进程峰值，更不是生成程序的运行内存。','',
        '## 完整 API 请求','',
        '| 场景 | 阶段 | 请求 | B 中位 ms | H 中位 ms | 配对 H/B | 探索性 95% |',
        '|---|---|---|---:|---:|---:|---|']
    for r in summary['cells']:
        lo,hi=r['exploratory_95']
        lines.append(f"| {r['case']} | {r['mode']} | {r['scenario']} | {r['medians']['B']/1e6:.3f} | {r['medians']['H']/1e6:.3f} | {r['paired_ratio']:.6f} | [{lo:.6f}, {hi:.6f}] |")
    lines+=['','## 门的分开裁决','',
        '| 范围 | 单元 | 区间低于 1 | 区间高于 1 | 跨 1 |','|---|---:|---:|---:|---:|']
    for label,rows in [('全部',allrows),('真实生产 owned/slices',production),('预登记 cold/整进程/Build',gate)]:
        c=census(rows);lines.append(f"| {label} | {c['cells']} | {c['interval_below_one']} | {c['interval_above_one']} | {c['interval_overlaps_one']} |")
    lines+=['','全部和真实生产子集均有回退；8 个完整进程区间没有一个完全低于 1。',
        '按原预登记保守拒绝准入，不用事后删除单元来制造通过。',
        '这批 7 对样本不证明 H 在所有环境都更慢；它足以使当前候选不能依据本批证据进入生产。','']
    (repo/'docs/research/093/ALL-CELLS.md').write_text('\n'.join(lines))

if __name__=='__main__':main()
