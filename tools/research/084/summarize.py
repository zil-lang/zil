#!/usr/bin/env python3
"""Keep every paired observation in compact, independently reviewable receipts."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import statistics

parser=argparse.ArgumentParser();parser.add_argument("--out",type=Path,required=True)
parser.add_argument("--evidence",type=Path,required=True);parser.add_argument("--tables",type=Path,required=True)
args=parser.parse_args();out=args.out.resolve();dest=args.evidence
read=lambda p:json.loads(p.read_text())
canonical=lambda v:hashlib.sha256(json.dumps(v,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()).hexdigest()
def sha(p):
    with p.open("rb") as f:return hashlib.file_digest(f,"sha256").hexdigest()
identity=read(out/"identity.json");samples=read(out/"samples.json");summary=read(out/"summary.json")
finish=read(out/"sample-finish.json");command=read(out/"sampling.command.json")
assert finish["processes"]==len(samples)==192 and finish["requests"]==768 and command["exit"]==0
portable=[]
for row in samples:
    p={k:v for k,v in row.items() if k!="records"}
    directory=out/"samples"/f"r{row['round']:02}"/f"{row['case']}-{row['mode']}-{row['variant']}"
    controls=read(directory/"controls.json")
    p["raw_report"]=str((directory/"report.json").relative_to(out));p["raw_report_sha256"]=sha(directory/"report.json")
    p["complete_control_sha256"]=canonical(controls);p["requests"]=[]
    for record,control in zip(row["records"],controls):
        report=record["report"]
        assert report["status"]=="Success" and report["verification_runs"]==1 and record["active_queries"]==0
        p["requests"].append(dict(scenario=record["scenario"],api_wall_ns=record["api_wall_ns"],
            compiler_report_seconds=report["seconds"],status=report["status"],phase=report["phase"],
            verification_runs=report["verification_runs"],active_queries=record["active_queries"],cached_queries=record["cached_queries"],
            query_dispositions=dict(Counter(e["disposition"] for e in report["query_events"])),
            query_events_sha256=canonical(report["query_events"]),input_fingerprint=report["input_fingerprint"],
            sources_sha256=canonical(record["sources"]),source_file_count=len(record["sources"]),
            scalar_interfaces_sha256=canonical(report["scalar_interfaces"]),complete_control_sha256=canonical(control),
            artifacts={k:dict(path=str(Path(path).relative_to(out)),sha256=sha(Path(path))) for k,path in report["artifacts"].items()}))
    portable.append(p)
for round_number in range(8):
    for case,mode in {(r["case"],r["mode"]) for r in portable}:
        pair=[r for r in portable if r["round"]==round_number and r["case"]==case and r["mode"]==mode]
        assert len(pair)==2 and {r["variant"] for r in pair}=={"baseline","candidate"}
        assert pair[0]["complete_control_sha256"]==pair[1]["complete_control_sha256"]
dest.mkdir(parents=True,exist_ok=True)
(dest/"performance-samples.json").write_text(json.dumps(dict(format="084-every-process-v1",baseline=identity["baseline_commit"],
    candidate=identity["candidate_commit"],schema=14,window=finish["window"],raw_samples_sha256=sha(out/"samples.json"),
    vmhwm_scope="Parent runner/compiler process across all four requests; excludes clang child peak.",processes=portable),indent=2)+"\n")
receipt=dict(task="084",baseline=identity["baseline_commit"],candidate=identity["candidate_commit"],schema=14,
    formal_status="Complete",production_decision="NoGo",author="/root/research084",independent_review="Pending",
    sample_command=command,finish=finish,retained_processes=168,retained_requests=672,warmup_processes=24,warmup_requests=96,
    full_summary_sha256=sha(out/"summary.json"),sample_receipt_sha256=sha(dest/"performance-samples.json"),
    statistics=summary,attribution="modules64 unchanged has no new raw publication; its observed gain cannot be credited to absent-key shortcut.",
    production_restoration_commit="3b00993c",measured_candidate_retained_in_git=True,
    decision_basis="Full-request gains are inconsistent, real-application gains are not established, and observed regressions are retained. No resampling to pursue significance.")
(dest/"performance.json").write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+"\n")
lines=["# 084 全部正式配对结果", "", "各单元保留7对；比值为逐对candidate/baseline的中位数，与两列中位数相除不同。小于1表示候选较快；区间为预登记的探索bootstrap，不能当产品门。", "",
    "## 完整请求API", "", "| 语料 | 模式 | 场景 | 基线中位ms | 候选中位ms | 配对比 | 探索95%区间 |", "|---|---|---|---:|---:|---:|---|"]
for c in summary["cells"]:
    lines.append(f"| {c['case']} | {c['mode']} | {c['scenario']} | {c['medians_ns']['baseline']/1e6:.3f} | {c['medians_ns']['candidate']/1e6:.3f} | {c['paired_ratio']:.4f} | [{c['exploratory_95'][0]:.4f}, {c['exploratory_95'][1]:.4f}] |")
lines += ["", "## 四请求完整进程及峰值", "", "父进程VmHWM为四请求合计高水位，不是每阶段独占，也不含外部clang子进程。进程wall含报告/源身份记录、编辑恢复及会话销毁。", "",
    "| 语料 | 模式 | 基线中位ms | 候选中位ms | 配对比 | 探索95%区间 | 基线/候选VmHWM中位KiB |", "|---|---|---:|---:|---:|---|---:|"]
for c in summary["processes"]:
    lines.append(f"| {c['case']} | {c['mode']} | {c['medians_ns']['baseline']/1e6:.3f} | {c['medians_ns']['candidate']/1e6:.3f} | {c['paired_ratio']:.4f} | [{c['exploratory_95'][0]:.4f}, {c['exploratory_95'][1]:.4f}] | {statistics.median(c['vmhwm_kib']['baseline']):.0f} / {statistics.median(c['vmhwm_kib']['candidate']):.0f} |")
lines += ["", "逐进程/逐请求的原始数值、每次父进程峰值、输出身份与完整报告位置见[evidence/impl/084/performance-samples.json](../../../evidence/impl/084/performance-samples.json)。完整原始包包含所有阶段文件、Build可执行文件、源码和预热。", ""]
args.tables.parent.mkdir(parents=True,exist_ok=True);args.tables.write_text("\n".join(lines))
print(json.dumps(dict(status="Pass",processes=len(portable),requests=sum(len(p["requests"]) for p in portable),cells=len(summary["cells"]))))
