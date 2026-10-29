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

## CP04：独立的原生运行质量对照

`runtime_bench.py` 单独回答编译产物的运行质量，四步为 prepare、build、measure、summarize。它继承已冻结 CP01 计划的五个 **base ZIL 源码**，逐文件校验后复制；复用并冻结 210 的 `tree_expected` 和 retained 独立 Python reference。它不修改 `bench.py` 的编译性能口径。

输入长度在看到双方运行结果之前固定。四个人工树的每次执行各调用 8,388,608 次生成的标量函数，另有模块、循环、effect 和标准输入输出调用；这是源码计数，不能当作实测指令数或耗时。retained 仍运行实际消费者语料。

| 语料 | 每次输入字节 | 权重 |
| --- | ---: | ---: |
| tree4x2 | 1,048,576 | 5% |
| tree32x8 | 32,768 | 15% |
| tree256x8 | 4,096 | 30% |
| tree1024x8 | 1,024 | 30% |
| retained | 262,144 | 20% |

build 用双方冻结编译器各生成五个可执行文件，明确 O0、LLVM API、1 CGU、1 worker，不启用对象缓存和阶段计时。该步骤保留编译过程收据，但编译耗时不进入运行质量分数。每个可执行文件先跑一次空输入启动诊断，再跑一次完整输入正确性/预热；这 20 次预检也不进入正式样本。空输入耗时单列，不从正式时间中扣除，不用于删除样本。

默认五个 ABBA/BAAB 配对区块，每个区块随机排列五语料，合计 **100 次正式原生执行**；`--blocks 3` 仅供另立计划时选择。每次使用同一个固定 CPU，stdin/stdout/stderr 均连接文件；完整进程 wall 与 wait4 CPU 包括启动、运行及标准输入输出。输出必须逐字符合独立 oracle、stderr 为空、退出码为零，且输入和可执行文件运行前后身份一致。超时杀死该进程组，失败行和已有输出保留；不会用成功重试替换失败。

汇总先平均同一区块、同一语料、同一臂的两次观察，再取区块中位数，以表中权重合并绝对耗时，最后计算 B/A。原始大值仍保留并参与配对 bootstrap 重采样；不筛选离群值。CPU 与 wall 分列，whole-block bootstrap 区间只作探索性描述。输入长度、I/O 占比和共享宿主限制了结论范围；这是固定工作负载的运行质量检查，不能据此宣称全部应用更快，也不能替代产品门或 Rust 对照。

```sh
python3 tools/research/compile-performance/runtime_bench.py prepare \
  --out /tmp/zil-runtime-plan --corpus-plan /path/to/cp01-plan/plan.json \
  --baseline /path/to/zil-cp02 --candidate /path/to/zil-cp04 \
  --baseline-source 9614aac90ee58359fef05da4dcfe92011b7c91c2 \
  --candidate-source CANDIDATE_SHA \
  --clang /path/to/clang --tool /path/to/libLLVM.so \
  --cpu CPU_ID --blocks 5

python3 tools/research/compile-performance/runtime_bench.py build \
  --plan /tmp/zil-runtime-plan/plan.json --out /tmp/zil-runtime-build

python3 tools/research/compile-performance/runtime_bench.py measure \
  --plan /tmp/zil-runtime-plan/plan.json \
  --build-receipt /tmp/zil-runtime-build/result.json \
  --out /tmp/zil-runtime-measure --quiet-window

python3 tools/research/compile-performance/runtime_bench.py summarize \
  --plan /tmp/zil-runtime-plan/plan.json \
  --build-receipt /tmp/zil-runtime-build/result.json \
  --records /tmp/zil-runtime-measure/records.jsonl \
  --out /tmp/zil-runtime-summary.json

python3 tools/research/compile-performance/test_runtime_bench.py
```

先提交工具，再 prepare。没有可评分的候选时不得预填通过状态。measure 的 `--quiet-window` 表示协调者已停止并发编译/测试；工具无法保证共享宿主独占。任一构建/预检失败、日志或身份漂移、正式行失败、缺行或完成收据不匹配，均拒绝评分。计划修订和重跑使用新目录，保留旧失败记录。

保存冻结源、输入、oracle、10 个可执行文件、构建/运行收据和所有完整 stdout/stderr。构建生成的 `.ll` 仅在成功获取哈希后删除，不清理其它目录；默认正式 stdout 总量约 27 MB。完成校验后可以无损压缩原始日志，复核前应解压恢复同名内容。工具测试以小输入、模拟编译/执行与人工时间覆盖冻结、oracle 拒绝、身份漂移、超时、完整性、配对统计，以及编译/启动成本不混入正式运行评分；另用两个微型进程检查文件收发和进程组超时回收。它们不是新的性能实验。
