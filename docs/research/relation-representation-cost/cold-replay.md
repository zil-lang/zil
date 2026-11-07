# 063 冷恢复：完整关系重放

本例只重建研究工具并重放完整关系，不重新采集正式性能。Git 历史中必须存在生产基线 `70d3a729a6db6f8355ea99942e6f05207eb8815b` 和研究源码 `1c679ad8dd864f17a1a952404fd66e1b3c7e3c57`；文档/证据交付提交可以更晚。不能从当前已集成新语义的主树直接构建，再把结果标成这次冻结实验。

先按主项目交付说明恢复 Git bundle、Rust 1.99.1 与离线 Cargo 缓存，在恢复的 Git checkout 根目录执行以下命令。`cut063` 只做前端与关系实验，不调用 clang；复现完整 pipeline/native 实验还需要冻结的 clang 23.1.5。文件系统路径会进入普通日志，但不进入这里的规范关系字节。

```bash
set -euo pipefail
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=1
ZIL_063_REPLAY="$(mktemp -d /tmp/zil-063-replay.XXXXXX)"
mkdir "$ZIL_063_REPLAY/source"
git archive 70d3a729a6db6f8355ea99942e6f05207eb8815b |
  tar -x -C "$ZIL_063_REPLAY/source"
git archive 1c679ad8dd864f17a1a952404fd66e1b3c7e3c57 \
  tools/research/063 tests/perf/impl063 docs/research/063 \
  changes/IMPLEMENTATION-0063.md |
  tar -x -C "$ZIL_063_REPLAY/source"
cargo build --offline --locked --release \
  --manifest-path "$ZIL_063_REPLAY/source/tools/research/063/Cargo.toml" \
  --target-dir "$ZIL_063_REPLAY/target" --bin cut063
"$ZIL_063_REPLAY/target/release/cut063" \
  "$ZIL_063_REPLAY/source/tests/perf/impl059/independent.zil" \
  mono:owner:gc "$ZIL_063_REPLAY/mono" > "$ZIL_063_REPLAY/mono.json"
"$ZIL_063_REPLAY/target/release/cut063" \
  "$ZIL_063_REPLAY/source/tests/perf/impl059/independent.zil" \
  fused:owner:gc "$ZIL_063_REPLAY/fused" > "$ZIL_063_REPLAY/fused.json"
python3 - "$ZIL_063_REPLAY" evidence/impl/063/cold-replay.json <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
contract = json.loads(pathlib.Path(sys.argv[2]).read_text())
source = root / 'source' / contract['input']
assert hashlib.sha256(source.read_bytes()).hexdigest() == contract['input_sha256']
for method in contract['replay']:
    tag = 'mono' if method['method'].startswith('mono:') else 'fused'
    result = json.loads((root / (tag + '.json')).read_text())
    assert result['method'] == method['method'] and result['schema'] == 9
    assert len(result['functions']) == len(method['functions'])
    for expected, actual in zip(method['functions'], result['functions']):
        for key in ('id', 'name', 'outcome', 'rows', 'candidate_work'):
            assert actual[key] == expected[key], (tag, key, actual, expected)
        candidate = (root / tag / expected['candidate_file']).read_bytes()
        oracle = (root / tag / expected['oracle_file']).read_bytes()
        other = (root / ('fused' if tag == 'mono' else 'mono') /
                 expected['candidate_file']).read_bytes()
        assert candidate == oracle == other
        assert hashlib.sha256(candidate).hexdigest() == expected['canonical_sha256']
print('Pass: 2 methods x 3 functions; complete canonical bytes match raw oracle and each other')
PY
```

`f0=consume`、`f1=independent`、`f2=main` 分别有 8、1440、13 个完整联合关系行。每个文件包含 schema、事件标签、原始程序点、fault/abort/exit 联合状态；不是边缘分布或仅摘要。候选自身还执行独立 raw oracle、精确基数与全成员核对。independent 函数关系文件为 110,460 字节，SHA256 为 `57d1efbf002dbab5fa96d203e20a576c6e18e2b1dae81c652ee37ecbd9cdc9b2`。

同一源码和参数下，`independent` 的完整候选 work 应分别为 19,586,288 和 20,647,385。时间、源码绝对路径和重建二进制 SHA 不要求与原路径构建相同；不能据此改写原正式计时身份。受测环境下 GC work 的新进程确定性另有 N−1/N/N+1 测试；Rust 标准库实现改变时应明确新的工具链身份。

原始归档 `portable-raw-063.tar.gz` 解开后是 `research/063/`，适合在新 checkout 的 `out/runs/` 下恢复。归档保留 `development`、`frozen-001`、`frozen-002`、`frozen-003`，包含原命令、失败日志、完整关系、实际 dump/native 产物和正式样本；排除可重建的 `source`、`lazy-source`、`target`、`lazy-target`。`RAW-MANIFEST.json` 逐文件记录字节 SHA256，外层归档 SHA 和文件数见 `evidence/impl/063/acceptance.json`。恢复后应按 manifest 核字节；不要要求原命令中的历史绝对路径仍存在。
