# CP13：O2 并行配置预检与单 CGU 定位

2026-10-30，执行者 `/root/llvm_strategy`。本收据只记录正确性、配置和一次阶段定位，不报告加速比，也不构成产品门通过。

## 固定身份和范围

- 生产源码：`e8e274c1b28509547a4854d46708ae1601e756e4`。
- 预检二进制：`combined-e8e274c1`，SHA-256 `3ae400c2eaa83d5643362b5cd1aa29915844904fe95ee01d69900b2a2612b0ba`。
- runner：`542d30676c42e1b691287bcab55874799ca910c1` 的 `rust_bench.py`；完整 runner/helper/tool/source 身份保留在证据 plan 中。
- LLVM 23.1.4，ZIL `default<O2>`，8 CGU、4 jobs；Rust 1.99.1 原生 CGU 配置，原 runner 的溢出、panic、调试和 bitcode 选项保持。
- 五语料和固定权重未变；CPU affinity 为 0、1、2、3。无编译缓存，不声称清除了 OS 页缓存。
- 只运行 `prepare` 和 `preflight`。未运行任何 `measure`、`measure-runtime` 或 `summarize` 评分命令。预检原始记录含运行时间，但这些值不进入正式比较。

## 正确性结果

Rust/ZIL 共 10 次构建均通过，32 个短输入 oracle、10 次启动校验、10 次预热校验均通过。每个 ZIL 请求恰好执行一次独立 Verify。实际 CGU 数小于请求数时使用原有分区结果，不人为拆分小语料。

| 语料 | ZIL 实际 CGU | 实际 jobs | fallback | 短输入校验数（两臂合计） |
| --- | ---: | ---: | --- | ---: |
| tree4x2 | 7 | 4 | 无 | 6 |
| tree32x8 | 8 | 4 | 无 | 6 |
| tree256x8 | 8 | 4 | 无 | 6 |
| tree1024x8 | 8 | 4 | 无 | 6 |
| retained | 3 | 3 | 无 | 8 |

完整证据在 `evidence/research/compile-performance/CP13/e8-o2-8cgu4jobs-preflight/`。冻结 plan SHA-256 为 `939b83b046e5747a2aa7e7aee96d9c7de08cf5b53d68e9bcfcc1653bc7b435ff`；作者机器可读总收据为 `CP13/preflight-author-result.json`。

可复用原始目录为 `/tmp/cp13-e8-o2-8cgu4jobs-plan` 和 `/tmp/cp13-e8-o2-8cgu4jobs-preflight`。10 个可执行文件保留在后者的 `executables/<case>-A`、`executables/<case>-B`，每个哈希见 `artifacts.json`。`records.jsonl` 中的 `work-*` 路径是已清除的构建暂存目录，不应作为后续运行质量输入。可执行文件大小本身不能证明两种语言的运行质量相同。

## 单次 O2 大树定位

另用固定 `combined-e8e274c1-timings` 对相同 tree1024x8 基准源码执行一次 O2、1 CGU、1 job 的完整构建。该请求成功、Verify 为 1，原始 invocation、二进制哈希和完整 trace 在 `CP13/e8-o2-1cgu-diagnostic/`；工作目录 `/tmp/cp13-e8-o2-1cgu-diagnostic` 保留产物。

| 阶段 | 单次 wall 时间 |
| --- | ---: |
| 完整 compiler request | 12.966 s |
| backend（含下列子阶段） | 11.708 s |
| machine lowering | 0.397 s |
| LLVM 文本渲染 | 0.121 s |
| machine preflight | 0.116 s |
| LLVM Builder | 0.146 s |
| LLVM 优化 | 3.247 s |
| LLVM object emission | 7.238 s |
| link | 0.251 s |

父 span 包含子 span，不能将表中各行相加。此次请求的主要耗时是对象代码生成，其次是 LLVM 优化。单次 trace 只用于确定研究方向，不能替代冻结配置的配对正式采样。

## 待授权的内部化边界

源码审阅发现 `emit.rs` 的普通 `zil.fn.<id>` 定义目前统一为 External。当前 Typed/LIR 函数没有可直接用于该策略的公开导出元数据；仅看入口枚举不足以推导任意外部调用者都不存在。

拟议实验使用默认关闭的显式 closed-executable 选项，只进入 Linux 完整 build，保留公开 emit/raw LLVM API 的外部符号合同。先执行现有 CGU 分区，再从精确 LIR 非 foreign 函数 ID 形成候选白名单；被其它 unit 的真实语义引用使用的定义仍为 External，只有其余候选定义可改为 Internal。入口包装器、外部声明、运行时辅助函数和 globals 保持。该次序避免提前内部化导致分区器把全调用图合并。

单 CGU Clang 快路径目前消费分区前文本，实验实现必须同步消费变换后的唯一 unit。对象缓存对完整 machine Module 计算键，实际 linkage 也必须继续进入键。实验保留 `default<O2>`、全部前后验证以及错误发布边界，收益和运行质量另行验证。

LLVM 的 [linkage 规范](https://llvm.org/docs/LangRef.html#linkage-types) 和 [GlobalDCE 说明](https://llvm.org/docs/Passes.html#globaldce-dead-global-elimination) 支持“内部化可允许删除不再被引用的残余定义”这一机制判断；它在这些语料上是否减少优化或对象生成时间仍是待检验假设。Rust 的 [codegen 选项说明](https://doc.rust-lang.org/rustc/codegen-options/index.html) 也明确 CGU 并行与生成代码优化之间存在取舍，因此配置试验必须附带运行质量比较。
