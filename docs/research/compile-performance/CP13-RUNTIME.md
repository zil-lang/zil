# CP13：直接对照产物运行质量

`runtime_bench.py` 继续只对编译好的两个 ZIL 产物进行完整原生进程计时；编译、空输入启动诊断和长输入 warmup 不进入运行得分。五语料、原权重、固定长输入、独立 oracle、单 CPU 和 ABBA/BAAB 规则不变。

本次增加显式 `--profile O0|O2` 及每臂的 `--baseline-units/jobs`、`--candidate-units/jobs`、`--baseline-closed-executable`、`--candidate-closed-executable`。默认仍为 O0、单单元、单 worker、关闭 closed 模式。计划及摘要保存完整 `build_config`，build/preflight 保留实际 argv；后续核对 argv 与冻结配置逐项一致。closed 模式还必须由实际 compiler JSON 的 `native_codegen.closed_executable` 确认；缺失、误报或字符串替代布尔值不计通过。

这允许把新的封闭可执行配置直接与旧的完整 O2 单单元产物配对，而非用不同时间窗口的两个 Rust 比值间接推断运行质量。示例（路径和精确源码提交须替换，准备时工作树必须干净）：

```sh
python3 tools/research/compile-performance/runtime_bench.py prepare \
  --out /tmp/cp13-quality-plan --corpus-plan /path/to/frozen-zil-plan/plan.json \
  --baseline /path/to/old-zil --baseline-source EXACT_OLD_COMMIT \
  --candidate /path/to/new-zil --candidate-source EXACT_NEW_COMMIT \
  --clang /path/to/clang --tool /path/to/libLLVM-23.so --tool /path/to/ld.lld \
  --profile O2 --baseline-units 1 --baseline-jobs 1 \
  --candidate-units 8 --candidate-jobs 4 --candidate-closed-executable \
  --cpu 0 --blocks 5

python3 tools/research/compile-performance/runtime_bench.py build \
  --plan /tmp/cp13-quality-plan/plan.json --out /tmp/cp13-quality-build

# 停止全部编译/测试后，独立执行原生运行窗口。
python3 tools/research/compile-performance/runtime_bench.py measure \
  --plan /tmp/cp13-quality-plan/plan.json \
  --build-receipt /tmp/cp13-quality-build/result.json \
  --out /tmp/cp13-quality-runtime --quiet-window

python3 tools/research/compile-performance/runtime_bench.py summarize \
  --plan /tmp/cp13-quality-plan/plan.json \
  --build-receipt /tmp/cp13-quality-build/result.json \
  --records /tmp/cp13-quality-runtime/records.jsonl \
  --out /tmp/cp13-quality-summary.json
```

本工具的 build 用运行窗口指定的单 CPU，构建耗时仅为非评分元数据，不能拿来和四 CPU 的完整构建成绩比较。被编译程序的 CGU 配置明确冻结，两臂产物在运行阶段均固定同一个 CPU。旧窗口的源码、计划、原始记录和旧 runner hash 保留；新增工具版本不能被用来改写旧成绩。

原有 14 项工具控制加 4 项配置、模式报告及相对路径控制，共 18 项通过。这只验证工具控制，不是实际运行性能结果。运行退化须保留并阻止将对应配置宣布为总体收益；有限语料仍不代表整个语言或行业负载。
