# CP08：真实 Rust 完整构建与运行质量对照

## 目的与范围

旧 CP03 的 `bench.py` 比较两个 ZIL 二进制，记录 Rust 版本并不意味着实际比较过 Rust。本工具固定 **A=Rust、B=ZIL**，实际执行两种编译器生成可执行文件，并另开窗口测量产物运行成本。历史 210/212/CP03/CP04 文件不修改，旧成绩不归到 LLVM23。

入口为 `tools/research/compile-performance/rust_bench.py`，仅依赖 Python 标准库，要求 Linux x86_64、`wait4` 和 CPU affinity。复用 210 成对真实源码及独立 Python oracle、212 完整进程计时/加权算法、CP04 原生进程运行/输出核对。工具控制测试不构成编译性能证据。

## 冻结合同

| 维度 | 实际规则 |
|---|---|
| 默认语料 | tree4x2、tree32x8、tree256x8、tree1024x8、retained，权重5%/15%/30%/30%/20% |
| 源码 | 同一生成器的ZIL/Rust，base/private/public三个快照；每个文件和oracle冻结SHA-256 |
| 优化 | O0、O2分别开计划；不合并评分 |
| cold | 每条请求新工作目录；双方都不请求源码/对象/增量缓存；仅base冷请求 |
| incremental | 每个序列从空缓存开始，随后未变、私有编辑、公开编辑；权重10%/35%/40%/15%；Rust增量与ZIL未核源候选/对象缓存均显式开启 |
| Rust主要配置 | 不传codegen-units，保留实际工具链默认；matched只用于单独诊断 |
| 算术与目标 | 两端原生x86_64；Rust显式target-cpu=x86-64、overflow-checks=on、panic=abort、debuginfo=0、embed-bitcode=no |
| ZIL | 显式LLVM API、固定CGU/jobs、阶段诊断关闭；每次verification_runs必须为1 |
| 对象命中 | incremental首请求必须0命中、未变必须全命中；不能静默回落后仍视为通过 |
| 链接 | Rust默认采用工具链原生选择；可另立计划用--rust-linker指定driver；ZIL保存每次实际clang/link argv |
| 构建计时 | compiler新进程启动到完整exe完成，包含当前核验、lowering、机器码、链接与发布；不算Cargo no-op，不以阶段差额代替完整墙钟 |
| 进程成本 | wait4墙钟与user/system CPU；RSS为单进程最大值，不宣称同时进程树峰值 |
| 采样 | 三或五ABBA/BAAB配对区块，区块内语料顺序固定随机种子；先平均同臂两次，再跨区块中位，再加权绝对成本，最后算B/A |
| 预检 | 每语料A/B各一套完整请求；base产物另跑空输入启动诊断与长输入warmup，均不进入得分且不扣除 |
| 完整性 | 工具、源码、输入、oracle、日志、产物或行序变化，任一失败/超时/缺行均禁止评分；不重试覆盖旧目录 |

`--tool`至少显式传入实际LLVM动态库，可重复补充其它依赖。prepare自动冻结Rust sysroot中的std/alloc/core等库、rustc_driver/LLVM动态库和bundled linker，同时记录PATH内cc/gcc/ld/ld.lld/as及clang旁的LLD候选。Rust `-Vv`保存其内部LLVM版本；不能把ZIL LLVM23.1.4称作Rust同一个LLVM版本。系统libc/crt、动态加载器与OS属于共享宿主依赖，未在本工具内重编，也未宣称完整供应链鉴定。

源码提交由调用者声明并校验Git对象，二进制另行哈希；二进制无法反推源码来源，所以必须同时保留实际构建收据。工具拒绝dirty工作树prepare，之后固定runner/所有helper/oracle字节；新实现或新参数必须创建新计划。

## 运行质量

preflight在独立目录保存每个base的Rust/ZIL可执行文件、编译日志、所有短输入oracle结果、空输入诊断和长输入warmup。后续 `measure-runtime` 只启动这些冻结产物，不调用编译器；仍使用同样的配对区块和语料权重，固定一个CPU。

长输入沿用CP04：四个树分别1,048,576/32,768/4,096/1,024字节，对应约8,388,608次生成的标量函数调用；retained为262,144字节。这是源级调用估计，不是硬件计数。每次完整native进程包括启动和文件I/O，stdout逐字核对独立oracle，stderr必须为空、退出码必须0。编译耗时、启动诊断和warmup不混入运行分数。

构建与运行使用独立命令、原始记录及摘要；必须同时检查，不能把运行退化交换成编译提速。运行窗口测的是base及上述固定输入，不声称全部错误/分配失败路径等价。

## 执行

先构建并保存ZIL候选、提交runner。以下路径和提交替换为实际值，保留同一shell环境。O0/O2、cold/incremental分别使用不同计划目录。默认全部五语料；`--case tree4x2`仅供首轮smoke，摘要会记录原权重覆盖，不把子集冒充全部。

```sh
python3 tools/research/compile-performance/test_rust_bench.py

python3 tools/research/compile-performance/rust_bench.py prepare \
  --out /tmp/cp08-o0-cold-plan \
  --zil /path/to/zil --zil-source EXACT_ZIL_COMMIT \
  --rustc /path/to/rustc --clang /path/to/clang \
  --tool /path/to/libLLVM.so \
  --profile O0 --cache-mode cold --rust-profile native \
  --units 1 --jobs 1 --blocks 3

python3 tools/research/compile-performance/rust_bench.py preflight \
  --plan /tmp/cp08-o0-cold-plan/plan.json \
  --out /tmp/cp08-o0-cold-preflight
```

协调者停止其它编译/压力测试后运行；`--quiet-window`是协调者的明确声明，工具不能保证共享宿主物理独占。

```sh
python3 tools/research/compile-performance/rust_bench.py measure \
  --plan /tmp/cp08-o0-cold-plan/plan.json \
  --preflight /tmp/cp08-o0-cold-preflight/result.json \
  --out /tmp/cp08-o0-cold-build --quiet-window

python3 tools/research/compile-performance/rust_bench.py summarize \
  --plan /tmp/cp08-o0-cold-plan/plan.json \
  --preflight /tmp/cp08-o0-cold-preflight/result.json \
  --records /tmp/cp08-o0-cold-build/records.jsonl \
  --kind build --out /tmp/cp08-o0-cold-build-summary.json

python3 tools/research/compile-performance/rust_bench.py measure-runtime \
  --plan /tmp/cp08-o0-cold-plan/plan.json \
  --preflight /tmp/cp08-o0-cold-preflight/result.json \
  --out /tmp/cp08-o0-cold-runtime --quiet-window

python3 tools/research/compile-performance/rust_bench.py summarize \
  --plan /tmp/cp08-o0-cold-plan/plan.json \
  --preflight /tmp/cp08-o0-cold-preflight/result.json \
  --records /tmp/cp08-o0-cold-runtime/records.jsonl \
  --kind runtime --out /tmp/cp08-o0-cold-runtime-summary.json
```

preflight、measure和measure-runtime都使用新目录，已有目录拒绝覆盖。正式计时后逐条执行原生正确性核对不进入构建计时；每条compiler stdout/stderr和native stdout/stderr完整保留。只有临时编译工作目录清理，冻结语料、源hash、原始日志和preflight产物保留。无完成收据或任何记录失败均不能汇总有效分数。

## 结论限制

默认80%权重是合成调用树，不能称作行业分布。retained使用Rust Vec与ZIL ByteBuffer，现有对照显式保持增长行为，但正向输出一致不证明所有OOM、panic、失败清理或整个语言安全等价。Rust使用预编译标准库，ZIL链接宿主C库，这两者的构建不在本项目编译窗口中；两端运行时启动/链接成本均在相应真实窗口内。

O2冷构建与O2增量是两种不同工作流；Rust官方提示增量可能抑制部分优化，因此二者分别报告，不把较慢配置拼接成有利参照。Rust原生配置与matched配置也不能逐语料择差混分。原生linker与显式driver的结果各自绑定计划，不把换链接器归因为前端改进。

三/五区块bootstrap区间只描述本次共享宿主样本，产品门始终NotRun。工具交付和13项控制通过只证明本工具受测行为，不表示ZIL已经超过Rust。实际性能结论必须另附完整新窗口和非作者复算。

一手配置依据：

- https://doc.rust-lang.org/rustc/codegen-options/index.html
- https://doc.rust-lang.org/cargo/reference/profiles.html
