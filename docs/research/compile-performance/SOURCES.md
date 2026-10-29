# 编译性能专项参考来源

检索日期：2026-10-29。以下为一手文档/源码入口，当日读取的是移动的 main/在线文档，未获得的上游 commit SHA 不伪填。未复制外部源码正文；后续若直接移植代码，另冻结实际上游提交和许可。外部倍率不计为 ZIL 成绩。

| 来源 | 可借鉴机制 | 本专项适用边界 |
|---|---|---|
| [Rust 增量查询](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html) | 记录真实依赖；重算后结果未变可阻断失效传播；机器工作产物与普通查询值分开 | CP01只优化现有失效索引，不宣称实现完整按项red-green，也不复用Verify资格 |
| [Rust profiling](https://rustc-dev-guide.rust-lang.org/profiling.html) | self-profile/measureme、原生profiler与Cargo图分别观察不同层 | CP02阶段诊断与CP03无探针性能分开；rustc自身LLVM占比不能外推ZIL |
| [Cargo timings](https://doc.rust-lang.org/cargo/reference/timings.html) | 构建依赖关键路径、等待/活跃工作、jobserver | CGU工人和外部构建应共用预算；不能将Cargo no-op与编译器完整请求比较 |
| [Rust codegen options](https://doc.rust-lang.org/rustc/codegen-options/index.html) | CGU粒度影响并发、增量与优化质量 | 保留默认1CGU控制；Rust对照同时记录默认实用配置与固定配置 |
| [Rust单态化/CGU](https://rustc-dev-guide.rust-lang.org/backend/monomorph.html) | 稳定非泛型代码与易变化实例的分区 | 后续研究稳定声明/实例身份；不能仅改数字函数名便声称完整稳定缓存 |
| [Rust后端协调源码](https://github.com/rust-lang/rust/blob/main/compiler/rustc_codegen_ssa/src/back/write.rs) | WorkItem、jobserver、ThinLTO及缓存工作产物 | ZIL已有按估计成本降序的CGU排程，不重复作为新成果 |
| [LLVM ThinLTO](https://clang.llvm.org/docs/ThinLTO.html) | 摘要索引、跨模块导入、并行后端和增量缓存 | 待研究多CGU的O2质量；不预设冷编译加速，不把原生对象缓存等同ThinLTO缓存 |
| [Blaze架构](https://github.com/indexable-inc/blaze/blob/main/docs/architecture.md) | 持续查询数据库、函数机器码产物、拼接与链接 | 当前文档记录2026-10-27转向自行完整构建，旧Cargo接管段落明确属旧模型；不能再仅称Cargo包装器 |
| [Blaze工具链](https://github.com/indexable-inc/blaze/blob/main/docs/toolchain.md) | 自带固定语言/标准库/ABI身份 | 比较须记录实际工具链，不能假定使用项目原Rust pin |
| [Blaze函数级增量实录](https://github.com/indexable-inc/blaze/blob/main/docs/reports/INCREMENTAL.md) | 函数相对语法、语义与绝对源码位置解耦；缓存驱逐对回溯验证的影响 | 稳定身份/按项查询后续候选；不能把数据库查询数减少当端到端收益 |
| [Blaze缓存](https://github.com/indexable-inc/blaze/blob/main/docs/cache.md) | 函数机器码/重定位缓存、稳定名字、工具链和构建身份 | 长期进程内复用与跨进程磁盘复用不同；源码变动仍可能改crate及其依赖身份 |
| [Blaze基准](https://github.com/indexable-inc/blaze/blob/main/docs/benchmarks.md) | 冷/热/多类编辑、原生运行和进程树内存分列 | 不能只取热编辑宣传倍率，不能忽略运行质量与常驻内存 |

## 本仓复用与新实验身份

CP03导入 `tools/research/210/corpus.py` 生成五语料，保留其调用树及已有retained消费者；Python行为oracle来自 `tools/product/109/reference104/run.py`。计时/源码同步/绝对成本加权借用 `tools/research/212/bench.py` 的独立函数。新runner的prepare同时冻结这些脚本、oracle合同、消费者输入、生成语料和二进制的SHA256；本轮没有改写210/212或把其旧成绩计入新结果。

公开事实只支持研究方向。本轮CP01的实际收益、回归、CP02计时覆盖及产品状态由新提交上的真实收据裁决。
