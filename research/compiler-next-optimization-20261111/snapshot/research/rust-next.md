# 从 Rust CGU 到 ZIL：下一步应验证真正的 ThinLTO 导入

核查日期：2026-11-11。结论：**优先做一个独立、显式启用的 clang 23 / lld 23 ThinLTO 端到端研究原型；暂不另造机器 IR 的导入大小规则。** 原型应把现有唯一归属的函数模块交给 LLVM，由真实 summary 决定导入并执行跨模块优化，同时缓存 pre-link bitcode 与 ThinLTO 后端产物。它能消除 ZIL 前端无条件复制完整调用闭包这一机制，但能否保住运行质量、是否减少入口 LLVM 工作，必须由真实 import list 和运行门证明；不能把接口可用等同于收益已证。

本轮只读源码和官方文档；未构建、运行样例、改动生产代码。上轮“入口 68 个 canonical 定义 + 504 个导入定义”是父任务已完成的实测输入，本轮不重新计算其原始数据。

## 1. 精确来源

| 对象 | 固定版本 / commit | 本文用途 |
|---|---|---|
| Rust 当前 main | [9de746938314271db6dbb62dc3b4f07288d07d54](https://github.com/rust-lang/rust/commit/9de746938314271db6dbb62dc3b4f07288d07d54)，2026-11-11 | 当前 partition、实例归属、内联准入、LTO 与复用逻辑 |
| Rust 1.99.1 | [58cf8f9c35b355199727627d07ae34a4e2846d67](https://github.com/rust-lang/rust/commit/58cf8f9c35b355199727627d07ae34a4e2846d67) | 对照上轮 Rust 工具链；partitioning 主干机制仍存在，不能把当前 main 当成上轮运行证明 |
| rustc-dev-guide | [13085df8b3590c62a9b7c91a535d82c1d7611acf](https://github.com/rust-lang/rustc-dev-guide/blob/13085df8b3590c62a9b7c91a535d82c1d7611acf/src/backend/monomorph.md) | collector → partition → codegen 的开发指南 |
| LLVM / clang / lld | [llvmorg-23.1.4 / 7a941dfb2877a0b8cf30323fefa85ad019e0b0f5](https://github.com/llvm/llvm-project/commit/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5) | ZIL 对接的 C API、ThinLTO pre-link、import 与 cache 实现 |
| ZIL | [99035931d358add0eaed6d4535035c38413701dd](https://github.com/zil-lang/zil/commit/99035931d358add0eaed6d4535035c38413701dd) | 本地只读核对现有 builder、partition、clang 分支与验证边界 |

下文 GitHub 链接均固定到精确 commit。未用第三方对 Rust / LLVM 行为的概括作为实现依据。

## 2. Rust 的三个不同单位

### 2.1 MonoItem 决定“实例是什么”，CGU 决定“谁生成它”

Rust collector 先收集需要单态化的项目，再 partition；函数项目是具体的 Instance，另有 static 和 global asm，不能简单等同源码函数名。InstantiationMode 划分为 GloballyShared 与 LocalCopy：前者有共享生成位置，后者可在引用它的 CGU 中生成私有实现。导出、extern indicator、entry、inline 属性及 drop glue 等参与该判断。参考 [mono.rs 的定义与选择](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_middle/src/mono.rs#L27-L234) 与 [collector UsageMap](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_monomorphize/src/collector.rs#L255-L307)。

这与“每一个调用目标都递归复制进调用者 CGU”有实质区别。partitioner 先安置 GloballyShared roots；随后只沿 UsageMap 标为可内联的 LocalCopy 边递归，遇另一个 root 不继续穿透。复制项目按 CGU 去重并使用 Internal linkage。[实际实现](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_monomorphize/src/partitioning.rs#L219-L312)

不能照搬这种私有函数副本的身份政策：Rust 明确允许同一个函数因不同 CGU 得到不同函数指针，也可能让不同函数因合并而相等；ZIL 当前 canonical 地址与共享状态约束必须独立保持。[Rust fn_addr_eq 官方契约](https://doc.rust-lang.org/std/ptr/fn.fn_addr_eq.html)；ZIL 当前 builder 也明确不增加 unnamed_addr、保持 canonical 地址身份，见 [native.rs](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-llvm-api/src/native.rs#L369-L390)。

### 2.2 stable / volatile 是初始归属，不是编辑频率预测

启用增量时，Rust 按 characteristic source module 安置 root，并把 generic function 的 monomorphized 实例放入 volatile CGU；非泛型主体留在相应 stable CGU。characteristic module 不是固定文件编号，trait / impl 方法还参考其 Self 类型的归属。命名包含 crate 身份、模块路径与专用后缀。[分配及 characteristic 规则](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_monomorphize/src/partitioning.rs#L638-L749)；[CGU 名字构造](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_middle/src/mono.rs#L616-L702)。

收益来自把“新增某种泛型实例”与“改变普通模块主体”分开。它并不保证一个模块最终恰好两个输出文件：超出 codegen-units 时还要确定性合并，实际合并参考复制项目的加权重叠，兼顾大小，减少重复的优化与输出。[合并实现](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_monomorphize/src/partitioning.rs#L317-L382)

因此，stable / volatile **不能直接解释或消除 ZIL 上轮纯多文件函数主体编辑导致的 572-body 入口**；那里并非新增泛型实例造成的 ownership 污染。把模块改名或另设 volatile 桶，不会自动减少入口实际可见的 504 个导入主体。

### 2.3 前端复制少，不等于关掉所有跨 CGU 优化

Rust 的 cross_crate_inlinable 在处理明确属性及特殊函数之后，若发现启用 incremental，就不再对未注解函数推断额外内联资格；源码说明额外内联会增加小编辑的工作。[cross_crate_inline.rs](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_mir_transform/src/cross_crate_inline.rs#L18-L100)

但优化、多 CGU 且未显式要求其他 LTO 时，当前默认仍可启用本 crate 内的 ThinLocal；incremental 不是这里的禁止条件。单 CGU、O0、目标不支持和明确禁用等另算。[session.rs 的完整 LTO 决策](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_session/src/session.rs#L996-L1067)

应学习的是分层：前端拥有实例和声明，LLVM 后端再决定有价值、合法的跨模块可见性。直接减少前端可见主体却不补上真实跨模块优化，不能由“Rust 也少复制”证明质量安全。

## 3. ThinLTO 实际导入什么、怎么复用

### 3.1 summary 决策与实际主体导入是两个阶段

clang 的 ThinLTO pre-link 使用专门的优化管线，然后写带 summary 的 bitcode。thin link 合并 compact summaries，计算 import / export；后端才读取和导入所需函数主体并继续优化、生成对象。summary 不是能够替代函数执行语义的独立缓存对象。[clang 官方固定版本文档](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/clang/docs/ThinLTO.md)；[BackendUtil.cpp 的 pre-link 与 writer](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/clang/lib/CodeGen/BackendUtil.cpp#L1128-L1188)。

LLVM import list 以来源 module、全局 GUID、导入种类为单位；候选选择会检查合法性、指令数量、AlwaysInline / NoInline，并使用调用边及 hotness 等信息。不是所有可达函数的完整闭包，也不是 ZIL machine cost 的同一个阈值。[FunctionImport.h](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/Transforms/IPO/FunctionImport.h#L342-L366)；[候选筛选实现](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L298-L343)。

Rust 自己在 C++ wrapper 里调用 ComputeCrossModuleImport，后端执行 promotion、weak resolution、internalization 和真实 import 后再做 ThinLTO 优化。[PassWrapper.cpp](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_llvm/llvm-wrapper/PassWrapper.cpp#L1367-L1540)；[Rust 后端顺序](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_codegen_llvm/src/back/lto.rs#L712-L791)。

### 3.2 缓存单位仍是模块后端工作，不是任意函数级命中

Rust 先确认此前有对应 work product、当前 CGU 可复用；ThinLTO 还重新计算当前索引和每模块 LTO key，仅当该模块本身可复用且新旧 LTO key 一致时，才复用 post-LTO 对象。[前端 CGU 复用](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_codegen_ssa/src/base.rs#L1218-L1261)；[ThinLTO 二次确认](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_codegen_llvm/src/back/lto.rs#L472-L529)。

LLVM23 的 LTO key 包括编译器版本、代码生成及优化配置、当前模块 hash、exports、实际 imports 的 GUID / 种类，**也包含每个实际导入来源模块的 hash**，另有链接解析信息。因此从模块 X 导入函数 f 后，X 中另一个未被导入的函数 g 发生改变，也可能让调用者的后端缓存失效。这条路线没有承诺精确到函数主体的最小失效。[LTO.cpp 缓存键](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTO.cpp#L138-L255)

对 ZIL 的含义：移除前端完整复制是确定的结构变化；“一次叶函数编辑不再重编入口”不是确定结果。若 LLVM 仍从全部叶模块导入函数，入口仍可能全失效，只是输入和优化组织不同。

## 4. 一个最小合法的端到端原型

### 4.1 先走固定 clang / lld，避免第一步维护 C++ LTO 驱动

LLVM23 有 llvm-c/lto.h 的 thinlto_create_codegen / add_module / process / cache_dir 等 C 接口，技术上可以直接对接；但这是 libLTO 接口，不应假定当前链接的 libLLVM 已导出它。还要管理输入 buffer 生命周期、唯一 module ID、外部保留符号、目标设置、错误与线程配置。[C API](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm-c/lto.h#L660-L892)

另一个容易漏掉的障碍是：LLVMWriteBitcodeToMemoryBuffer / File 仅调用普通 WriteBitcodeToFile，并未传入 summary index。不能简单把当前 C API module 写成 bc 就称其为 ThinLTO 输入。[BitWriter.cpp](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Bitcode/Writer/BitWriter.cpp#L20-L49)。clang 已实现 pre-link pipeline 和 summary/hash writer，lld 已实现逐符号 prevailing / visible-to-regular-object 等 resolution，首片使用这些实现更小，也更容易保存实际证据。[lld resolution](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/lld/ELF/LTO.cpp#L239-L293)

### 4.2 数据路径

1. 当前完整源码检查、Verify、machine 原模型验证继续完成；同一 canonical 函数或可变 global 仍只有一个 owner。沿用当前 local/address 引用并组、声明合法性、entry/wrapper、ABI 和 .lc1 名字选择，研究入口只关闭预复制的调用主体，不改变这些约束。
2. 每个 owner 模块保留本模块定义与必要外部声明，通过现有 structured Module.to_text() 导出 IR。ZIL 已有这一输出及分单元 clang 分支，无须为了第一片先新增 LLVM C++ bridge。[build.rs](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/build.rs#L450-L695)
3. 固定 clang23 对每个当前验证过的输入执行 `--target=x86_64-unknown-linux-gnu -x ir -O2 -flto=thin -c owner.ll -o owner.thin.bc`。不要先跑现有 default<O2> 再把结果包装成“同一条 O2”；这是独立 pre-link / post-link 组合。
4. 固定 clang23 驱动固定 lld23 链接这些 bitcode 与必要的普通 runtime / foreign objects；使用 `-flto=thin -O2 -fuse-ld=lld -Wl,--lto-O2,--thinlto-jobs=J,--thinlto-cache-dir=...`。必须解析并记录实际 lld 路径，不能仅凭 -fuse-ld=lld 字符串认为已 pin。先保留 save-temps / index / import 证据与 optimization remarks；具体诊断参数以该23.1.4二进制支持为准。
5. 独立缓存 pre-link bc：key 必须覆盖当前 owner model、目标/ABI、确切 clang 二进制和完整 pre-link 参数；ThinLTO native cache 交给 lld。外层 cache namespace 同时绑定 clang / lld / libLLVM 文件身份与项目协议，不能混用原 object-cache key。
6. 产物验证和发布沿用当前原子提交边界，失败 raw / 命令 / stderr 先保存。限制整个请求的总 CPU 预算；pre-link workers 与 ThinLTO workers分阶段，不能各自默认吃满四核后再把并发请求乘上去。

官方 ThinLTO 文档给出 compile/link 的 -flto=thin、lld native cache 和线程控制；lld 固定源码也明确把 thinLTOJobs 传给 backend 并调用 localCache。[文档](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/clang/docs/ThinLTO.md#controlling-backend-parallelism)；[lld 后端](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/lld/ELF/LTO.cpp#L180-L209)；[cache](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/lld/ELF/LTO.cpp#L338-L383)。

### 4.3 建议写集与范围

首个 prototype 只选 x86_64 Linux executable、多文件、显式研究开关；普通构建路径保持默认。建议写集：

| 文件 | 最小职责 |
|---|---|
| crates/zil-backend-llvm/src/machine_cgu.rs | 新研究入口复用原 ownership / 并组 / 声明逻辑，明确只输出 canonical owners，不复用旧 imports/layout 语义的 cache key |
| crates/zil-backend-llvm/src/build.rs 或独立 thin_lto.rs | 经完整检查后导出 owner IR；调用固定 clang / lld，分阶段、捕获证据、限制线程、取消和清理 |
| crates/zil-backend-llvm/src/lib.rs | 仅在研究工具需要时暴露受限入口；不扩大现有默认语义 |
| research/thin-lto/driver.py | 工具 SHA pin、pre-link cache、完整命令、阶段耗时与缓存/import 收据；如 orchestration 已在 Rust 实现，此文件不必存在 |
| backend 专项 tests / research fixtures | 两叶模块+入口最小端到端、符号身份、外部 effect、坏 model 和缓存反例；保留当前全验证 |

第一片可由研究导出器+外部 driver 完成，不必先新增全套用户 CLI。若要接当前 LLVM API 原生请求的 preflight，需在导出入口明确调用其现有验证函数，不能因最终由 clang 解析而跳过 ZIL model 的约束。当前 build.rs 明确拒绝 Clang backend 的 BeforeAndAfter；AfterOptimization 通过 -enable-merge-functions=true 且 aliases=false 传给 clang。因此首片只能明确支持 After 或 Disabled，并分别记录 pre / post-link merge 设置；BeforeAndAfter 需要额外受审实现，不能悄悄降级。[当前分支和参数](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/build.rs#L245-L260)；[clang_function_merging](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/build.rs#L972-L982)。

## 5. 主要障碍与机械收益边界

**Linkage / identity。** 不得把 canonical 变成私有副本、添加 unnamed_addr 或启用 ICF 类地址合并假设；地址取用、跨模块 local global、foreign callback、递归 SCC 与 wrapper 都必须保持当前身份。lld 的 internalization 依据真实链接 resolution，只能在可见性证据允许时发生；不可用“它是可执行文件”替代未知 foreign 使用的保留信息。第一片保留当前 local definition 与其 users 的并组，宁可形成大模块，也不要凭名称重造跨模块 local linkage。

**O2 质量。** LLVM 的选择性导入有真正的跨模块优化能力，因此比前端硬截断更值得验证；它仍可能少导入、少内联或不同地合并函数。不能保证当前完整可见性下的代码质量。当前 full O2 与新 ThinLTO O2 应有不同 pipeline ID；每个 arm 的 clean 与 cached 产物先证明一致，跨 arm 允许不同码但先过现有运行质量门。

**缓存 /验证。** 已命中 pre-link cache 不允许跳过当前声明、全模型或当前 source Verify；命中只能跳过经身份绑定的后端工作。bitcode 必须来自本次已验证 model 或完全匹配的受信缓存。固定 clang 的 pre-link 及 lld 的 post-link verifier，保存阶段输出/验证收据；仅最终能链接不是前后 IR 验证的替代。

**收益可机械确定的部分。** 前端不再为入口无条件创建504个函数体副本；未改 owner 的 pre-link 工作可以命中；没有被 LLVM 实际导入的模块无需通过这一 imports 关系牵动入口后端。**不能机械确定的部分**包括全请求 wall、cold CPU/RSS、入口 post-link 优化量和 runtime：多了 clang 进程、pre-link优化、thinlink和重新读取 bitcode；LLVM仍可能导入多数主体。原入口68canonical也包含其它源文件owner，某些叶编辑仍会直接改变入口owner，这一合桶事实不会因 ThinLTO消失。

因此先复用现有 ownership 做单变量原型。如果实际 import 仍覆盖全部叶模块，或入口本身 owner 合桶使主要编辑始终直击入口，就记录此边界；不要把同时更换分区身份、数量和选择性导入包成一个已经归因的收益。

## 6. 最小反例与停止条件

| 控制 | 要证什么 |
|---|---|
| 同一叶模块：改被导入函数、改未导入函数；另一未导入模块改主体 | 从真实 import list 和 module hash 推导 post-link hit/miss；保留 LLVM 源模块粒度的保守失效，不伪造函数级命中 |
| direct call + address-of 同一函数、两 canonical 函数同主体、共享可变 global、foreign 读写与 callback、递归 SCC | 输出及外部效应与 main 一致；地址身份、唯一状态和可调用 ABI 不被内部化/合并破坏 |
| 增删不相关声明、坏的未使用声明、重复定义、缓存里旧签名/旧目标/旧工具 | 当前完整检查仍拒绝；缓存身份变化产生 miss 或明确失败，不复用陈旧许可 |
| 热循环跨文件调用与上轮八个动态 runtime case | 六排列、独立 oracle、每 case B/A≤1.10 且 Rust/B≥.90；相对 main 的退化不能由仍快于 Rust 抵消 |
| 冷启动、暖 CLI、resident，以及已有 k1 / 多文件 edit | 所有源变更后一次 compile；完整请求计时，分列 pre-link / thinlink / import / optimize / codegen / native link，固定总CPU，保留失败和长尾 |

第一道停止条件是正确性和 runtime；过不了就保存 NoGo，停止后续规模矩阵。第二道是机制：在上轮代表性 N64 edit 上，记录 canonical owner 数、真正 imported GUID / 来源模块、pre-link hit/miss、post-link cache hit/miss 和入口 LLVM 耗时。若504份前端副本消失但入口实际后端工作没有减少，不把“导出更小”当作用户收益。只有两道门都过，再衡量完整请求的冷/暖和多任务表现。

**路线选择：实施一个真实 ThinLTO 的小片验证，暂不把 stable / volatile 分桶单独作为当前无泛型多文件主体编辑的生产优化。** 它验证的是“保留所有权和身份，把可见性选择与二次复用交给LLVM”是否适用于ZIL；无采样结果前没有采用结论。

