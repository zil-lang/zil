# 编译专项的冻结比较工具

`bench.py` 使用 Python 标准库；复用并冻结 210 的语料/独立输出 oracle，以及 212 的 Linux 进程计时、源码同步和加权聚合函数，不修改旧实验。默认五语料：tree4x2、tree32x8、tree256x8、tree1024x8、真实 retained 消费者；权重 5%、15%、30%、30%、20%。默认分别测 check 和 build、O0、1 CGU/1 工人，三个 ABBA/BAAB 区块；可选五区块。

## 边界

每次请求都会启动新的 CLI 和 CompilationSession。一个序列按冷进程、未变、私有编辑、公开编辑执行，复用本序列的磁盘未核源码候选；它不是持久会话增量、Cargo no-op 或清空 OS 页缓存。CP01 跨请求反向索引的算法探针另行报告；本工具只回答完整 CLI 成本及回归，不保证触发该优化的全部路径。

四请求权重依次为 10%、35%、40%、15%。每个配对区块先平均同臂两次结果，跨区块取中位数，按冻结权重合并绝对成本，再算 B/A；不平均加速比。check、build 分别汇总。`--case` 和 `--phase` 可缩小首轮：选择在 prepare 时冻结，记录原权重覆盖及归一化权重，不事后剔除失败或慢单元。

完整 CLI wall、Linux wait4 CPU 和原始输出逐行保存。CPU 包含 CLI 及其已回收子孙进程，不能代表外部守护进程；RSS 只记录 wait4 的单进程最大值，不冒充同时进程树峰值。原生输出校验在编译计时之外，保留每次 stdout/stderr，核对 210 独立 Python oracle。check 不冒充原生执行。

每四请求序列结束才清理本工具新建的工作目录（复制源、缓存、可执行文件）；冷到热序列中不清理缓存。冻结语料、所有原始日志、源码及机器产物摘要留存。中断留下成功前缀；失败/超时/输入变化/缺行均禁止汇总评分，补跑须新计划/新目录。非独占共享宿主的三/五区块 bootstrap 区间仅探索性描述，不升级产品门。

## 使用

先提交 runner，并各自构建、保存基线/候选；准备阶段不构建编译器。`--baseline-source` / `--candidate-source` 是调用者声明的源码提交，工具核实 Git 对象、冻结 crates tree 和二进制 SHA，但不能从二进制反推构建来源，须同时保存真实构建收据。用重复的 `--tool` 绑定实际加载的 LLVM 动态库、链接器；未传时不宣称完整运行依赖已鉴定。

```sh
python3 tools/research/compile-performance/bench.py prepare \
  --out /tmp/zil-cperf-plan \
  --baseline /path/to/zil-A --candidate /path/to/zil-B \
  --baseline-source BASE_SHA --candidate-source CANDIDATE_SHA \
  --clang /path/to/clang --tool /path/to/libLLVM.so \
  --blocks 3 --profile O0

python3 tools/research/compile-performance/bench.py preflight \
  --plan /tmp/zil-cperf-plan/plan.json --out /tmp/zil-cperf-preflight

python3 tools/research/compile-performance/bench.py measure \
  --plan /tmp/zil-cperf-plan/plan.json \
  --preflight /tmp/zil-cperf-preflight/result.json \
  --out /tmp/zil-cperf-measure --quiet-window

python3 tools/research/compile-performance/bench.py summarize \
  --plan /tmp/zil-cperf-plan/plan.json \
  --records /tmp/zil-cperf-measure/records.jsonl \
  --out /tmp/zil-cperf-summary.json
```

prepare 后保持工具、辅助脚本、语料、环境及 CPU/内存配额不变；新工具或计划修订开新目录。计时窗口不得同时编译/测试其它工程。默认对象缓存关闭；`--object-cache` 对 A/B 同时开启，不把缓存开关混入 CP01。阶段计时探针不通过本 runner 偷开，归因另跑。检查工具自身控制：

```sh
python3 tools/research/compile-performance/test_bench.py
```

这些控制使用人工时间和模拟执行失败，只检验统计、拒绝评分和日志保留；不是 ZIL 编译/性能证据。
