from pathlib import Path
import subprocess
import json
import hashlib

BASE = Path('/workspace/scratch/39be3faa4442')
REPO = BASE / 'zil-restored/wt177'
RUNS = BASE / 'run-logs/177'

def read(path):
    return json.loads(path.read_text())

def git(*args):
    r = subprocess.run(['git', '-C', str(REPO), *args], check=True, capture_output=True, text=True)
    return r.stdout.strip()

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

full = read(RUNS / 'workspace03/command.json')
lint = read(RUNS / 'clippy-final01/command.json')
fmt = read(RUNS / 'fmt-final01/command.json')
for run in [full, lint, fmt]:
    assert run['status'] == 'Pass' and run['exit_code'] == 0 and run['source_clean']
    assert run['source_commit'] == full['source_commit']
assert git('rev-parse', 'HEAD') == full['source_commit']
assert not git('status', '--porcelain')
git('merge', '--no-ff', '6db5f997627c4fc3cd796f0c803c6ee4aed3f9a1', '-m',
    'merge(review177): retain exact-source independent composition acceptance')
git('merge', '--quit')
assert not git('diff', '--name-only', full['source_commit'], 'HEAD', '--', 'crates', 'examples', 'tools')
subprocess.run(['python3', '/tmp/zil177-collect.py', '--repo', str(REPO)], check=True)

candidate = 'be6fd5cbf089072a13853cecb614abde2d05d58a'
consumer = read(RUNS / 'consumer-final01/result/summary.json')
prior = read(RUNS / 'consumer173-final01/result/summary.json')
ready = read(RUNS / 'readiness-final01/result/summary.json')
isolation = read(BASE / 'run-logs/compat177-by176/final-isolation.json')
assert consumer['status'] == prior['status'] == 'Pass'
assert consumer['source_commit'] == prior['source_commit'] == ready['source_commit'] == candidate
assert consumer['cli_sha256'] == prior['cli_sha256'] == ready['cli_sha256']
assert len(consumer['observations']) == 64
injections = [x for x in consumer['observations'] if x['mode'] == 'allocation_failure']
assert len(injections) == 28
assert all(x['allocation']['live'] == 0 and x['allocation']['allocated'] == x['allocation']['frees']
           for x in consumer['observations'] if 'allocation' in x)
groups = [[int(v) for v in row] for row in full['test_groups']]
counts = {'passed': sum(x[0] for x in groups), 'failed': sum(x[1] for x in groups),
          'ignored': sum(x[2] for x in groups), 'reported_groups': len(groups),
          'nonempty_groups': sum(sum(x) > 0 for x in groups),
          'zero_selection_groups_not_counted_as_pass': sum(sum(x) == 0 for x in groups)}
assert counts['passed'] == 1620 and counts['failed'] == counts['ignored'] == 0
reviews = []
for p in ['evidence/reviews/174/REVIEW.json', 'evidence/reviews/175/REVIEW.json',
          'evidence/reviews/176/REVIEW.json', 'evidence/reviews/177/public/REVIEW.json',
          'evidence/reviews/177/composition/REVIEW.json']:
    d = read(REPO / p)
    assert d['verdict'] == 'Pass'
    reviews.append({'path': p, 'sha256': sha(REPO / p), 'reviewer': d['reviewer'],
                    'implementation_commit': d['implementation_commit'], 'verdict': d['verdict']})

acceptance = {
    'schema': 'zil-language-stage-acceptance/177',
    'status': 'StagePass_FullZuiNotReady', 'actor': '/root',
    'implementation_commit': candidate,
    'last_production_change': '8abc75bbc1f98450a16efde00539d753fb836551',
    'public_implementation_commit': 'aa235a3379492221197b4686bec042c042c4e100',
    'final_workspace_source_commit': full['source_commit'],
    'final_workspace_crates_tree': full['crates_tree'],
    'candidate_to_final_crates_diff': git('diff', '--name-only', candidate, full['source_commit'], '--', 'crates').splitlines(),
    'reuse_reason': 'The only candidate-to-full-workspace crates change is the independent review177_composition test. Production, consumer sources and runner are identical. Final CLI was independently built by root and reviewer with identical bytes.',
    'cli_sha256': consumer['cli_sha256'],
    'target': 'x86_64-unknown-linux-gnu; LLVM',
    'schema_wire_version': 21, 'typed_fingerprint_domain': 'ZIL-TYPED-MODULE-V21',
    'spec_hash': 'c30c279a5ec0da4fec13637749df6d406daee84fbd9af9095eac88a59587e32f',
    'normative_hash': '8aecd0f052b3d8eb099d19d26cd0bad56b26d3a7be91c79aca5a962a010711be',
    'workspace': {'status': 'Pass', **counts, 'receipt': 'runs/workspace03/command.json',
                  'log': 'runs/workspace03/output.log'},
    'clippy': {'status': 'Pass', 'receipt': 'runs/clippy-final01/command.json',
               'scope': 'workspace, all targets, -D warnings'},
    'fmt': {'status': 'Pass', 'receipt': 'runs/fmt-final01/command.json'},
    'consumer177': {'status': 'Pass', 'builds': len(consumer['builds']),
                    'source_files': len(consumer['sources']),
                    'modules_per_entry': sorted({b['module_count'] for b in consumer['builds']}),
                    'native_executions': consumer['native_executions'],
                    'ordinary': consumer['ordinary_executions'],
                    'instrumented': consumer['instrumented_executions'],
                    'allocation_failure_injections': len(injections),
                    'all_live_zero_and_allocated_equal_frees': True,
                    'receipt': 'runs/consumer-final01/result/summary.json'},
    'consumer173_regression': {'status': 'Pass', 'native_executions': prior['native_executions'],
                               'ordinary': prior['ordinary_executions'],
                               'instrumented': prior['instrumented_executions'],
                               'receipt': 'runs/consumer173-final01/result/summary.json'},
    'readiness': {'status': ready['status'],
                  'original_probes': [{'capability': x['capability'], 'status': x['status'], 'phase': x['phase'],
                                      'diagnostic_codes': [d['code'] for d in x['diagnostics']]} for x in ready['gaps']],
                  'consumer164_native_executions': sum(x['executions'] for x in ready['native']),
                  'events_and_drops_per_execution': 1024,
                  'receipt': 'runs/readiness-final01/result/summary.json'},
    'independent_reviews': reviews,
    'supplemental_compatibility': 'compatibility-review/final-summary.json',
    'isolation': 'compatibility-review/final-isolation.json',
    'toolchain': 'runs/toolchain-final.json',
    'failed_attempts_retained': {
        'workspace01': '3 exact Analyze budget fixture failures; real extra type visits measured; no production budget raised',
        'workspace02': '1605 passed, 2 old Verify shape diagnostic expectations failed; original malformed operations still reject under precise migrated expectations',
        'consumer01_to_03': 'Generic Store builtin remains closed; concrete helper route and finite manages-field give/comparison propagation repaired',
        'composition_focused05': 'Missing owning-State indexed-profile guard; corrected before final full run',
        'composition_focused07': 'StableCrateId cache collision; 0 tests; fresh target used',
        'composition_focused08': 'Test used two State<F> inputs beyond existing callable signature profile',
        'composition_focused09': 'Partial whole-State overwrite correctly rejects unpaid old resource; assertion corrected without widening production',
        'cli_final01': 'Wrong Cargo package in command; no tests executed; final02 built zil-cli correctly',
        'public_frozen01': 'InvalidEvidenceIdentity correction retained; only exact aa235a3 frozen02 used for acceptance',
        'reviewers': 'Original author/reviewer fixture, parse and native attempts retained in their exact-source evidence directories'
    },
    'boundaries': [
        'Generic Store primitives remain closed in parametric bodies; concrete ordinary helper route is used.',
        'Callable F occurs in only one input parameter; this does not limit the number of FnMut invocations.',
        'Callback field loan remains conservative at whole-parent local granularity.',
        'Shared Store windows permit only admitted Copy reads; no mutable, direct capture, return or escaping window.',
        'Partial whole-State overwrite cannot discard unpaid old resources; Analyze still rejects it.',
        'Owned-State indexed non-Copy Package transfer remains closed; restored ordinary concrete-array compatibility is not a general indexed ownership protocol.',
        'Store profile is Linux x64 LLVM only; Win64 implementation and execution are not claimed.',
        'Original159 direct resource-array modes and direct E!ByteBuffer successful owned payload remain rejected.',
        'External retained-callback registration, shutdown/drain, reentrancy, cancellation and full UI are not established.'
    ],
    'packaging': {'source_contains_evidence': False, 'source_contains_execution_products': False,
                  'audit_separate': True, 'audit_limit_bytes': 50000000,
                  'actual_package_and_cold_results': 'Reported separately after packaging to avoid a self-referential hash cycle'},
    'full_language': 'NotSatisfied', 'product': 'NotRun', 'B0': 'Doing',
    'performance_measurement': False, 'new_rust_four_goal_conjunction_proven': False
}
assert acceptance['candidate_to_final_crates_diff'] == ['crates/zil-driver/tests/review177_composition.rs']
(REPO / 'evidence/impl/177/acceptance.json').write_text(json.dumps(acceptance, ensure_ascii=False, indent=2) + '\n')

doc = Path('/tmp/LANGUAGE-177-draft.md').read_text()
for k, v in {'FULL_COMMIT': full['source_commit'], 'FULL_CRATES': full['crates_tree'],
             'FULL_PASSED': str(counts['passed']), 'FULL_GROUPS': str(counts['nonempty_groups'])}.items():
    doc = doc.replace('@@' + k + '@@', v)
assert '@@' not in doc
(REPO / 'docs/LANGUAGE-177.md').write_text(doc)
next_doc = (BASE / 'run-logs/review177-package175/NEXT-LANGUAGE-178-draft.md').read_text()
start = next_doc.index('## 四任务与顺序')
next_doc = ('# 178–181 建议：先让已有 State 能写成可复用库\n\n'
            '本页是下一阶段建议，不是已派工、已实现或产品门通过记录。174–177 已完成有限 Store 字段、共享元素窗口、具体回调字段与真实 State 组合；精确受测身份和边界见 [本阶段报告](LANGUAGE-177.md)。完整语言 NotSatisfied、产品 NotRun、B0 Doing 保持。\n\n'
            + next_doc[start:])
next_doc = next_doc.replace('177真实组合与恢复检查完成后，已有能力足以开始一种受限的语言消费者：',
                            '177真实组合已完成，交付恢复结果另在旁置报告记录。已有能力可供一种受限的语言消费者起步：')
next_doc = next_doc.replace('再借用callback，再做容器修改', '再调用callback并结束该借入，之后修改容器或同父壳Copy状态')
next_doc = next_doc.replace('原159直接资源数组与错误载荷', '原159直接资源数组与E!ByteBuffer成功侧拥有返回')
next_doc = next_doc.replace('Linux结果不外推到Win64；', '当前Store机器profile只支持Linux x64 LLVM，其他target明确Unsupported；Win64仍需实现并验证。')
(REPO / 'docs/NEXT-LANGUAGE-178.md').write_text(next_doc)

readme = (REPO / 'README.md').read_text()
marker = '## draft23 施工恢复检查点'
assert marker in readme
top = '''# ZIL — 语言主线 174–177（2026-10-27）

本轮入口见 [174–177 实施报告](docs/LANGUAGE-177.md)：普通 State 拥有动态 Store、共享读取实际元素、保存并多次调用具体回调环境，以及三者的真实组合。内部 schema/raw Wire 为 **21**；完整 workspace **1620项通过、0失败、0忽略**，Clippy与格式检查通过。新组合完成64次原生执行，其中28次分配失败注入；上一阶段44次消费者也已重跑。完整语言 NotSatisfied、产品 NotRun、B0 Doing。源码包不放执行产物或证据；见 [交付与恢复](docs/DELIVERY.md)和[下一阶段建议](docs/NEXT-LANGUAGE-178.md)。

```sh
cargo build --offline --locked -p zil-cli
python3 tools/language/177/consumer.py --repo . --cli target/debug/zil --out out/language177-new-run
```

每次使用新的输出目录，消费者运行真实O0/O2及分配失败检查；它不代表完整GUI或产品验收。当前Store机器profile仅支持Linux x64 LLVM，元素窗口只读，callback贷款保守覆盖父local。泛型Store原语、精确独立字段借用、就地元素更新、Win64及外部保留回调关闭仍有明确前置。

此前 [170–173报告](docs/LANGUAGE-173.md)、[166–169报告](docs/LANGUAGE-169.md)和以下介绍保留各自历史身份、schema及受测数字，不代表本轮新增运行。

'''
(REPO / 'README.md').write_text(top + readme[readme.index(marker):])
delivery_path = REPO / 'docs/DELIVERY.md'
delivery = delivery_path.read_text().replace(
    '当前170–173交付延续同一格式，精确能力与验收见 [LANGUAGE-173](LANGUAGE-173.md)。源ZIP保持零证据/零执行产物，独立审计恢复ZIP与旁置报告分别提供。',
    '当前174–177交付延续同一格式，精确能力与验收见 [LANGUAGE-177](LANGUAGE-177.md)。源ZIP保持零证据/零执行产物，独立审计恢复ZIP与旁置报告分别提供。实际ZIP大小、SHA256、独立逐字节检查、全refs恢复及空target构建结果只读本轮旁置报告；上一轮已保存交付不重打。')
delivery = delivery.replace('当前入口：\n\n', '')
delivery = delivery.replace('--receipt evidence/impl/169/wire-02/command.json --receipt evidence/impl/169/wire-02/output.log',
                            '--receipt evidence/impl/177/acceptance.json --receipt evidence/reviews/177/composition/REVIEW.json')
delivery_path.write_text(delivery)
goals_path = REPO / 'docs/RUST-GOALS.md'
goals = goals_path.read_text()
anchor = '## 2026-10-27 语言170–173'
section = '''## 2026-10-27 语言174–177

State的直接Store字段、共享元素窗口和自含具体回调字段已组合为实际程序；最终workspace1620项通过，新消费者64次native含28次分配失败注入，旧消费者44次重跑，独立审核与原始反例分别保留。具体身份、首次失败、权限范围与精确预算迁移见[本阶段报告](LANGUAGE-177.md#rust-四目标)。

这证明有限语言片的组合行为、责任清理和实际分配平衡；不证明完整语言安全，也没有同语义Rust运行/内存、维护成本或编译成本比较。本轮没有编译性能采样；编译速度支线保持独立。泛型Store、精确callback字段来源、受限独占元素窗口、Win64实现与外部事件关闭仍未闭合。四目标合取和产品级可重复净突破均未证明；完整语言NotSatisfied、产品NotRun、B0 Doing保持，产品阈值原字节未改。

'''
assert anchor in goals
goals_path.write_text(goals.replace(anchor, section + anchor, 1))
git('add', 'evidence/impl/177', 'docs/LANGUAGE-177.md', 'docs/NEXT-LANGUAGE-178.md',
    'docs/DELIVERY.md', 'docs/RUST-GOALS.md', 'README.md')
git('diff', '--cached', '--check', '--', 'docs', 'README.md')
git('commit', '-m', 'docs(177): record verified State composition and compact delivery evidence')
assert not git('status', '--porcelain')
print(json.dumps({'root_delivery_commit': git('rev-parse', 'HEAD'), 'workspace': counts,
                  'source_crates_tree_unchanged': git('rev-parse', 'HEAD:crates') == full['crates_tree']}))
