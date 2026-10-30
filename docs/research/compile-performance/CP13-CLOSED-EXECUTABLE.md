# CP13：显式封闭可执行程序内部化

授权边界：`845cb23d8ffded03b3ea9511c198e63af33acd98` 的 CP11–13 计划。作者 `/root/llvm_strategy`。本提交只实现 backend 核心，默认关闭；CLI/driver 的字段传递由 CP12 作者另行提交，正式编译和运行质量采样由协调者进行。

## 需要解决的问题

现有普通 `zil.fn.<FunctionId>` 定义统一使用 External linkage。即使单 CGU 的 O2 已将调用内联，LLVM 仍须保留其可对外调用的定义。固定 e8 大树的单次诊断中，对象代码生成耗时 7.24 秒，优化耗时 3.25 秒；这是研究方向的依据，不是本改动的加速证据。原始诊断和五语料 8 CGU 预检见 [CP13-PREFLIGHT.md](CP13-PREFLIGHT.md)。

## 选项合同

`BuildOptions.closed_executable` 默认为 `false`。显式启用表示完整 Linux 可执行构建不承诺向外导出生成的 ZIL 函数身份符号。普通外部 C 声明、入口包装器、运行时辅助函数、全局对象以及函数地址的实际使用保留。

该策略只位于 `build` 系列的已验证输入路径。`emit`、`emit_with_options`、原始 `zil-llvm-api` 模型接口不读取此选项；公开 emit 的 `None`、`Main` 和 `ReturnValue` 包装器选择继续保持原有外部函数定义。未启用的 build 保留原符号行为。`EntryPoint::None` 仍返回原 `E-LINK-ENTRY`；显式模式对非 `x86_64-unknown-linux-gnu` 目标返回 Unsupported `E-CODEGEN-CLOSED-TARGET`。普通 Win64 构建不受该默认关闭的模式影响。

O0 和 O2 均继续使用原有 LLVM 管线与设置；本实验没有增删 pass 或弱化前后验证。

## 分区之后改变链接属性

1. 按原路径将精确的 VerifiedModule/LIR 降为机器模型，并保留原分区前文本作为诊断侧车。
2. 完成现有 source/linkage CGU 分区。该步骤不接收新的内部链接属性，因此不会因为新策略把整个调用图合并。
3. 从本次 LIR 的非 foreign FunctionId 生成精确符号白名单；不凭任意名称前缀推测可内部化的函数。
4. 扫描实际各单元的函数语义引用和 global initializer 引用，包括函数地址。其它单元引用的候选定义继续使用 External；仅其余有 body 的 External 候选改为 Internal。已有 local linkage、非候选项和所有声明保持。
5. 对实际变换后的每个单元执行原机器预检、LLVM Builder/优化/验证/对象生成，或原对象缓存命中后的完整本次机器预检。

重复候选定义的异常模型不执行变换，交给原机器预检拒绝。局部函数地址仍以原值被使用，没有额外 `unnamed_addr`、属性、调用约定或函数体修改。

单 CGU Clang 的原快路径过去直接消费分区前 `module.ll`。启用模式后，它改为写入并消费实际变换后的 `unit-0.ll`，仍由一次 Clang 进程编译和链接。多 CGU Clang/API 本来就消费计划中的单元，继续如此。

## 报告和对象缓存

`CodegenSummary` 增加 `closed_executable` 和 `internalized_functions`。前者表示本次显式策略，后者表示实际单元中改变的定义数量。公开 LLVM 侧车仍是分区前参考文本，不能用它推断每个实际 CGU 的链接属性。

对象缓存代码未修改。现有键包含完整机器 items 的派生 Hash，其中包含函数 linkage，并绑定当前编译器/LLVM/目标/优化身份。相同编译器下默认模式和内部化模式产生不同实际机器模型时，不能互相误命中；无实际模型变化时无需凭选项名称人为改变等价对象的键。前端检查点不因这个 Verify 之后的选项取得任何新的验证资格。

## 限定验证

机器模型测试覆盖精确非 foreign 白名单、名字相似但不在白名单的定义、函数地址的跨单元使用、global initializer 的跨单元使用、同单元的地址保留、所有非候选项不变、重复定义仍拒绝，以及现有分区控制。

实际 native 控制使用经独立 Verify 的手写 TypedIR，分别构建 Clang/API、单 CGU/请求 8 CGU、O0/O2、Main/指定 ReturnValue。三源组在启用前后仍为三个实际 CGU；`nm` 直接核对输出文件的全局符号，跨单元被调用者保留，只有可内部化的生成函数消失于全局符号表。指定入口的标准输出以及普通入口的返回值均核对。该 fixture 是 backend/ABI 控制，不冒充源语言验收语料。

同一对象缓存依次执行默认、内部化、内部化、默认，得到 miss、miss、hit、hit，且每次运行结果相同。None、Win64、伪造 LIR、Clang 启动失败等拒绝控制保留旧可执行文件和侧车。

最终有 LLVM API 的限定集为 6 个分区/内部化测试、7 个已有对象缓存测试和 5 个新 native 测试，全部通过；默认无 LLVM API 构建的 4 个适用 native 测试也通过。Clippy `--all-targets -D warnings`、fmt 和生产源码 whitespace 检查通过。原始日志和命令见 `evidence/research/compile-performance/CP13/core-*`。

## 当前限制

这是作者验证完成、等待非作者审核与正式采样的研究候选。保守保留跨 CGU 的符号可能留下已经无用的定义；本实验没有全程序 LTO、跨 CGU DCE 或改变默认 CGU 配置。正确性控制通过不意味着编译已经更快，也不意味着运行质量已经与默认模式相同。五语料完整构建及配对运行质量必须在冻结整合版本后另行核对。
