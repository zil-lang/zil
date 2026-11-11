# 请求内 canonical prelink 复用：C API 最小实现评估

结论：**技术上可行，值得先做受限研究实现；不是官方 ThinLTO importer，也尚无净编译收益证据。** 当前 LLVM 23.1.4 C API 已能运行 prelink/postlink O2、读写 bitcode、链接模块和逐 Use 核对函数引用，不需要 C++ 插件、额外 LLVM 实例、Cargo 依赖或共享 machine schema 改动。真正要新增的是两阶段 native API、同请求 prelink artifact、实际引用闭包/裁剪证明，以及 backend 的阶段屏障。不能在现有逐 unit `compile_module` 外简单包一层就得到它。

本次只读 main `99035931d358add0eaed6d4535035c38413701dd`、2549 proof 的现存工作树与官方 LLVM pin `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`，只写本文；没有修改生产源码、构建、运行 LLVM/native 或测量。旧 default/strict/private/identity 研究源码及 frozen 证据不变。

## 为什么现有结构能接入

`zil-llvm-api/src/native.rs::compile_input` 目前在一次调用中创建独立 Context/TargetMachine/Layout，完成 structured preflight → builder → LLVM verify → `default<O2>` → LLVM verify → object emitter。所有 raw handle 均为私有 RAII 类型，不能 Send/Sync；返回的只有对象字节与 receipt。`native/builder.rs::build` 已是可以复用的 structured 构造入口，无须回到文本 IR。[当前 native.rs](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-llvm-api/src/native.rs)

backend `build.rs` 已在 fold、partition、unit preparation 后形成完整 `Vec<UnitInput>`，`compile_units` 现在一阶段调度每个 unit 的完整编译。因此可在研究分支增加两阶段：先对全部 canonical units 并行 prelink 并收齐纯字节/事实，再并行生成各目的 unit。沿用 `codegen.jobs`，两个阶段不重叠，不叠加内外 worker 数；LLVM handles 始终留在创建它们的线程上。[build.rs](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/build.rs)

现有 API crate 已动态链接同一 `libLLVM-23`，BitReader/Writer/Linker 都是该 C ABI 的函数；不应新增 `-lLLVM` 静态库、自建库或 C++ 构建脚本。源码静读能确认 API 契约，实际 SDK DSO 符号可用性仍须由后续受控构建验证。

## 已核实的官方 API

| 功能 | 最小 API 与精确约束 |
|---|---|
| prelink O2 | `LLVMRunPasses(M,"thinlto-pre-link<O2>",TM,Opts)`；官方 alias 调用 `buildThinLTOPreLinkDefaultPipeline`。 |
| postlink O2 | `LLVMRunPasses(M,"thinlto<O2>",TM,Opts)`；官方 alias 明确调用 `buildThinLTODefaultPipeline(Level,nullptr)`，因此无 summary/index 的调用有官方实现路径；它不负责导入。 |
| 序列化 | `LLVMWriteBitcodeToMemoryBuffer(M)` 返回拥有的 buffer；复制成 `Vec<u8>` 后 dispose buffer。它只写普通 bitcode，不生成 ThinLTO summary/module hash，研究名称和 receipt 必须如实写明。 |
| 反序列化 | `LLVMCreateMemoryBufferWithMemoryRangeCopy` + `LLVMParseBitcodeInContext2(ctx,buffer,&module)`。这是 eager parse，**不消费 buffer**；与现有 `LLVMParseIRInContext` 消费 buffer 的规则不同，应分别使用 RAII。不要误用消费 buffer 的 lazy `LLVMGetBitcodeModuleInContext2`。 |
| 链接 | `LLVMLinkModules2(dest,src)` 成功返回 0，**无论成功失败都会消费 src**；调用前必须转移/置空源 Module RAII，失败则整个请求失败，不能继续使用可能部分修改的 dest。所有模块必须在同一个目的 Context 中解析。 |
| 裁剪 | 不选的 source definitions 设 Internal 后运行 `globaldce`；这只是实现工具，裁剪后仍需核对定义集合精确等于本 source 被选集合，不能把 DCE 当授权检查。 |
| 名称/引用 | 遍历 function/instruction/use，`LLVMGetFirstUse/NextUse/GetUser/GetOperandUse`；`LLVMSetValueName2` 和 `LLVMSetLinkage(...,8)` 实现原地 rename+Internal。 |

主源：[PassRegistry.def L281–290](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassRegistry.def#L281-L290)、[PassBuilderBindings.cpp](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderBindings.cpp#L52-L108)、[BitReader.cpp L58–73](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Bitcode/Reader/BitReader.cpp#L58-L73)、[BitWriter.cpp L43–49](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Bitcode/Writer/BitWriter.cpp#L43-L49)、[LinkModules.cpp L653–658](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Linker/LinkModules.cpp#L653-L658)。

### callee Use 判定可以由 C API 精确实现

LLVM23 `CallBase` 定义 callee 是最后一个 operand，CallInst 没有 subclass extra operands，bundles 排在参数之后。因此只接受满足以下全部条件的 Use `U`：

1. `LLVMIsACallInst(LLVMGetUser(U)) != null`，不接受 Invoke、CallBr、ConstantExpr、Alias、全局初始化器或未知 User。
2. `LLVMGetNumOperandBundles(call)==0`。
3. `LLVMGetNumOperands(call)==LLVMGetNumArgOperands(call)+1`，先检查范围与计数溢出。
4. `LLVMGetCalledValue(call)==F`，且 `LLVMGetOperandUse(call,numArgs)==U`。

必须逐 Use 检查，不能只看 User。`call @f(ptr @f)` 同一 CallInst 有 callee Use 和地址参数 Use，后一项会被第4项拒绝；nested Array/Gep、别名、store/return/freeze 也不能进入这个分支。direct graph 同样只记录 callee 是精确 Function value 的 CallInst；不 strip pointer casts，不凭 `zil.*` 前缀猜身份。

这是 pin 下的布局推导，需最近合法反例控制确认，不是跨 LLVM 版本承诺。主源：[InstrTypes.h L1192–1218、L1409–1433](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/InstrTypes.h#L1192-L1218)、[Core.cpp LLVMGetOperandUse / LLVMGetNumArgOperands](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/IR/Core.cpp#L1263-L1266)、[Core.h](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm-c/Core.h#L4150-L4234)。

## 最小合法执行链

### 第一阶段：每个 canonical unit 只 prelink 一次

沿用 2549 的显式 pure scope 与原 Complete/Disabled 比较，保持完整原 machine、fold 后 partition 输入、实际每个 unit 的 current preflight。原 Complete 的静默 copy fallback 仍靠 originalRequired==Allowed、canonical owner 唯一双射与原 function.cost checked-add 预算证明挡住，不能仅根据8unit/fallbackNone判断。

对每个 canonical unit 用现有 builder 构造 LLVM module，检查 target/layout，LLVM verify；运行 `thinlto-pre-link<O2>`，LLVM verify；遍历实际 module 收集 definitions、declarations、直接调用和逐 Use 事实，序列化普通 BC。预优化产生的新定义、global、alias、ifunc、未知外部/间接引用先失败关闭，不 strip 克隆后缀猜原函数。外部只接受原 exact declarations/signatures；LLVM intrinsic 只可按 `LLVMGetIntrinsicID` 非零且完整 LLVM verify 后认可，不用宽 `llvm.*` 前缀。

字节、事实、stage receipt 必须来自同一次 module，绑定 unit/canonical owner、原 machine proof、BC hash、target/cpu/features/PIC/default model、prelink pipeline 与 LLVM pin；不能把外部 JSON 的 Allowed 当 API 自带权限。此阶段不启任何磁盘/native cache。

### 第二阶段：目的 unit 组装与完整 O2

1. 从**实际 prelink** 全部 own definitions 出发遍历 direct-call 图，visited 去重并处理递归/SCC；跨 owner 目标构成 `Required_i`，要求每个目标有唯一 canonical body 且 `Required_i ⊆ Allowed_i`。原 helpers 已被本地内联时无需再导入消失的 helper；未知或缺 body 不能假装 foreign。
2. 目的新 Context 解析 own BC；按 source owner 把 Required 分组，每个 source 在本目的 unit 最多解析一次。own/source 都是已经验证的同请求 BC，但实际 parse 结果仍核 identity/target/layout/定义、调用事实与当前 proof。
3. source 被选 definitions 暂保 External 作为 DCE roots；其余 definitions 设 Internal 后 `globaldce`。闭包先证，DCE 后 verify，并要求实际全部 definitions **精确等于**本 source 的 selected names。若选中函数仍依赖未选本地 body，DCE 会把它保留，集合门必须失败，不可偷偷扩大名单。空 selected 不解析该 source。
4. 在同目的 Context 用 `LLVMLinkModules2` 链入，逐次核唯一 owner、没有重复/自动 `.1` 改名、未知定义、符号/类型/ABI或 module flags 冲突；最终 verify，核所有 Required 均有 body，额外 definitions 只能来自 own/Required。
5. 对实际新增 bodies 的每个 Use 做上述严格 direct-callee 检查；检查 `zil.import.<destination-unit>.<canonical>` 无碰撞后，只原地改 name 和 Internal linkage。**不增加 canonical 声明，不改 body/attrs，不调阈值、不设 alwaysinline。** 原递归/互递归的 Function operand 随原地 rename 保持正确。`setLinkage(Internal)` 会合法重置 visibility/DLL storage 并设置 dso_local，必须记录但不能随后 copy 回旧属性。[GlobalValue.h L547–555](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/GlobalValue.h#L547-L555)
6. verify → `thinlto<O2>` → verify → 复用当前 `LLVMTargetMachineEmitToMemoryBuffer`。保持 x86-64、空 features、PIC、default code model、O2 codegen、AfterOptimization/aliases=false；不使用 lld importer/index/cache。最终普通 native objects 交现有 clang 链接与原 staging 原子发布路径。

所有当前语言 Verify/machine preflight 留在真实请求内；LLVM verify 应覆盖 builder、prelink、裁剪、链接、私有改名和完整 postlink 后的实际 module。收据分开记录每个 checkpoint 的实际结果，不能拿 producer verify 或 rename-before-O2 verify 冒充当前完整 postlink verify。取消/任一阶段失败不发布对象/缓存，保留阶段与现有 partial 证据。

## 最小类型/API与预期写集

不改既有 `Config`、`Receipt`、`Object`、`model::Module` 或公共 CLI/schema。新增明确 research 模块与 additive API，普通入口不分叉：

- `PrelinkedUnit`：私有 BC `Vec<u8>`、actual facts、stage receipt；不包含 Context/Module/Value handles。只允许受检查的构造函数生成，getter 返回借用事实。
- `ImportSelection`：目的 unit、精确 canonical/Required/Allowed、原成本/owner proof 身份，均为 owned names/整数；backend 从当前 typed proof 构造，不从旧 cache 或环境 JSON 读取许可。
- `prelink_module(&Module,config,merging,cancelled)->PrelinkedUnit`。
- `compile_prelinked_unit(own,&all_prelinked,&selection,config,merging,cancelled)->ResearchObject`；`ResearchObject` 包现有 Object 和独立阶段 receipt，不能沿用 `default<O2>` pipeline identity 冒充新管线。

建议最小授权写集：`zil-llvm-api/src/lib.rs` additive 研究导出、`native.rs` 私有 target/verify/pass/emitter helpers、新增 `native/research_prelink.rs`；backend `build.rs` 研究门和两阶段调度、新增 backend 研究协调模块；复用 `machine_cgu/complete_visibility.rs`，仅加 typed 只读 accessors/结果类型与保持原 JSON receipt 的 renderer；必要的受限反例测试。`native/builder.rs` 应无需改，Cargo/根配置/共享 model/schema 不动。没有新公开 BuildOptions 字段，研究开关必须显式 opt-in，拒绝非 Linux x64/API/O2/incremental-after/pure、closed模式、object/native cache 和不匹配身份。

这里的“additive API”仍是跨 crate 的 Rust 接口改动，需 root 下一步明确授权；本报告没有提前实施。

## 成本机制与真实风险

确定能消除的是这条研究路径的 Python wrapper、跨进程 prelink/opt/dis/dump、反复大工具 hash、BC/IR 中间文件与真实 lld thinlink/index/importer；它们由同进程 C API、内存字节和实际结构遍历替代。完整语言/机器/LLVM验证、原 Complete proof clone、正常最终链接与诊断 material IR 仍在。不能从旧 wall 中算术扣去这些成本，也不能把旧 runtime Pass 当本路径同码或编译 Pass。

算法上 prelink 的 structured build/简化仅对 canonical 总输入做一次，重复导入体只在目的 postlink 阶段消费；但它新增 canonical prelink + full postlink 两阶段，donor 必须按目的 Context 重解析、DCE、链接。在最坏 U 个 units 都要彼此 imports 时，最多 U(U−1) 个 donor module parses，每次可能解析整 source unit，故解析/裁剪工作仍可达 O(U·总BC大小)，**不是函数级 O(Required) clone**。保留一个目标一次只持有一个 donor、完成链接即释放它，避免 U² 全量 module 同时常驻；BC总量只保一份，目的并发受 jobs 约束。第一次实现只证明请求内 reuse，不宣称跨请求 warm 命中。

8CGU并不必然使16处编辑全失效；按当前 pure fixture 文件分组，既有 k16 选择 N64偏到2个 owner、N256偏到1个 owner，连续16个文件则可覆盖8个。该路径首版既不启缓存也不测命中，未来 CGU prelink cache 必须按真实完整 model/ABI/属性/target key，不能按编辑文件数或单函数 body 猜依赖。[先前逐源码评估](prelink-cache-path-review.md)

主要风险是裁剪闭包、linker类型/flag合并、预优化产生的新符号、原地改名与地址身份、Call Use 布局、跨阶段所有权，以及内存/二阶段重复 IPO。全部可在当前 pure scope 保守拒绝；不应借此扩到有地址、可变 globals、closures/helper、异常、COMDAT、PGO/CFI 的通用模型。`thinlto<O2>` 无 summary 路径不会做 index-guided WPD/memprof，当前纯 direct scope不依赖它们，但这与真实 importer 并非同一生产能力。

最少的有效控制：原 planner silent fallback/预算 N−1、owner mismatch、Required 缺失/额外/未知、递归闭包去重、DCE保留未选body、链接重名/不同签名、合法 `call @f(ptr @f)` 地址反例、bundles/ConstantExpr拒绝、原地私有化后的 body/operand invariants、BitReader/Linker 成败各一次所有权释放、无 native feature 的构建路径。它们验证新增边界，不替代最后两个case的完整冷编译成本和运行质量门。

落地顺序：先 author 上述明确写集并静审；根单窗口构建/反例控制；原两case full request 编译+同数学 oracle/真实输出+原1.10/.90质量门；原生码可不同则记录差异，不能强称同码。只有质量和实际净编译成本都成立，才讨论跨请求缓存或扩大矩阵。
