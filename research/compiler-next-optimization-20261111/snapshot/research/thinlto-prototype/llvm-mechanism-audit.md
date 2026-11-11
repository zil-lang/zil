# LLVM 23.1.4 ThinLTO：导入、管线与质量诊断审计

## 当前结论与证据边界

ThinLTO能够把ZIL当前“分区前复制完整Internal实现，再逐CGU完整优化/发射”换为“canonical owner先做prelink简化，thinlink基于summary选择导入，再分布式postlink优化/发射”。它**可能**省未inline副本的后段优化与机器码发射，也增加summary/index、导入、bitcode与第二轮简化；不能从机制合法性直接推出完整请求提速。

质量有两个独立边界：LLVM默认导入仍是有门的选择，不保证完整call closure；被导入的External函数通常成为available_externally，它允许inline，但不具备原Internal私有clone的全部非inline IPO权限。因此，即使导入文件非空、完整body在IR里可见，也不能先宣称旧跨leaf SIMD、返回范围证明已恢复。也不能预设本轮失败，必须等冻结的n64/n256真实产物与两case运行门。

本报告只读取本项目源码、当前研究脚本和LLVM官方精确发布源码；没有改生产/脚本/协议，没有执行编译、链接、LLVM工具、native或计时。源码review和本报告均非运行证据，不与Builder84窗口重叠采样。前五节先于两case结果撰写，为待检验机制与诊断路径；第六节另记随后收到的Blaze源码补核与现有n64文本证据，不倒填为最初已知结果。

## 一、精确官方身份

已通过GitHub官方仓库commit接口把`llvmorg-23.1.4`解析为[`7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`](https://github.com/llvm/llvm-project/commit/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5)，与研究README的官方引用一致。以下全部定位到该commit，不使用滚动doxygen作为23.1.4的版本证明。

| 文件 | Git blob | 主要用途 |
| --- | --- | --- |
| `llvm/lib/Transforms/IPO/FunctionImport.cpp` | `032bdfb763e0a527263cad3fa7f53d358ebcf66f` | 导入阈值、合法性、工作队列、失败诊断与属性传播 |
| `llvm/lib/Passes/PassBuilderPipelines.cpp` | `5de45e5668fb92fa2df37ddfa1bc8ac851ef4dd5` | per-module/prelink/postlink的真实pass组装 |
| `llvm/lib/LTO/LTOBackend.cpp` | `73697a9d0d4465eac7fe3395d4802507a0b7092d` | rename/internalize/import/opt/codegen与save-temps对应 |
| `llvm/lib/Transforms/Utils/FunctionImportUtils.cpp` | `4f71239cda786da6b54eb4cdb245780a751e17ff` | 导入后的linkage和local promotion |
| `llvm/lib/Analysis/ModuleSummaryAnalysis.cpp` | `33caa268b1c6e26c8c0135ff12e37fadbe198e20` | summary导入资格的产生 |
| `llvm/lib/Analysis/ValueLatticeUtils.cpp` | `acc19667d7764989d6e7386b0928bb6531118466` | 跨函数参数/返回值跟踪权限 |
| `llvm/include/llvm/IR/GlobalValue.h` | `d7b0f1f25f92920a4a1ee3ab901f36947adc76df` | exact definition与available_externally的区别 |
| `llvm/lib/Transforms/IPO/SCCP.cpp` | `2ecadd529170f9051379e2e27efef50916987752` | IPSCCP消费上述权限并推导属性 |
| `lld/ELF/LTO.cpp` | `a11681133373d07440ef0e4393f9919226de9331` | LTO优化等级、vectorizer、PIC、remarks配置 |
| `lld/ELF/Options.td` | `64c42eb49607dc4f957bd9f5a31c8c9b73055e17` | 诊断选项的真实名称 |

实际工具是否就是这些发布字节，仍由本轮protocol的工具path/SHA、版本及build-binding证明；报告读了一个源码tag不等于校验SDK二进制本身。

## 二、真实函数导入门

### 默认不是完整closure

[FunctionImport.cpp:85–128](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L85-L128)定义普通初始`import-instr-limit=100`、普通递归因子0.7、hot递归因子1.0；hot/critical/cold的调用点乘数分别10/100/0，普通无profile边乘1。这里的大小是summary `instCount`，应读取**实际prelink后的LLVM IR**，不是原ZIL源码行数或旧bounded策略的machine cost32。

[selectCallee:235–342](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L235-L342)先要求live、合法function summary、非interposable、local GUID/module归属正确且未被标notEligible；随后`instCount > Threshold`才拒绝，所以等于阈值可以通过（选项文字的“less than”不能覆盖实际比较）。alwaysinline可绕大小门；noinline另行拒绝。当前研究没有授权新增alwaysinline或force-import-all，不能把这些特殊门当默认行为。

[computeImportForFunction:887–1073](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L887-L1073)沿summary的calls边挑选，目的模块已有定义直接跳过。每GUID记已考察阈值与导入/拒绝结果，同等或更小阈值不重复选择；更高阈值可重新考察并继续沿其callee走。普通深链按原当前Threshold乘0.7并截成unsigned，所以初始100往后通常为70、49、34……；不能把它描述成所有叶子共享一个全图预算。worklist为DFS式pop_back，但模块的每个live本地函数都先以100作为起点，实际某callee也可能从另一条更浅路径再次获得较大阈值。[模块初始化与遍历:1077–1117](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L1077-L1117)。

能导入不等于必inline：导入用summary规模门，后续inliner再按当时callee/caller IR、调用点与成本判定。反过来“未见独立函数定义”也未必未导入：它可能已inline或DCE，需对照`.3.import`与`.4.opt`。

### 合法性不能靠加阈值修复

[ModuleSummaryAnalysis.cpp:732–750](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Analysis/ModuleSummaryAnalysis.cpp#L732-L750)把不可重命名local、可能引用internal的inline asm、特殊block-address间接跳转、local ifunc等标为不适合导入；[1118–1161](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Analysis/ModuleSummaryAnalysis.cpp#L1118-L1161)还传播llvm.used local与不能promotion的refs/calls影响。普通pure fixture可能不触发，但不能据此扩大到address/global/helper输入并声称通用正确。

本轮先保留默认值，记录真实拒绝原因；`TooLarge`、`NoInline`、`NotEligible`、`InterposableLinkage`、`NotLive`、`LocalLinkageNotInModule`不能统称“inline失败”，更不能看到gate失败就扫阈值寻找好结果。

## 三、prelink/postlink与当前default<O2>的差别

当前main native.rs显式使用default<O2>、after MergeFunctions、loop interleaving/loop vector/SLP启用；每个已复制Internal import都进入同一CGU的完整优化。官方[per-module管线:1766–1803](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1766-L1803)依次跑module simplification与module optimization。

ThinLTO [prelink:1880–1946](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1880-L1946)调用module simplification，常规不接完整module optimization；官方注释明确把vectorization等膨胀阶段留到后面。但它仍有真实简化和内联，不能说prelink只产summary或完全不优化，某些loop简化中的unroll在无PGO条件下也仍可能执行。prelink内联可能缩小callee、消掉调用，也可能长大某个导出value函数，从而改变后续summary门与跨文件inline决策。

ThinLTO [backend:652–785](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTOBackend.cpp#L652-L785)按rename/promote、drop dead、summary finalize/属性传播、internalize、真正import，再优化和codegen。不是先把所有模块拼成一个普通default<O2>模块。实际优化入口是[buildThinLTODefaultPipeline:1949–2014](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1949-L2014)，在当前无context-profile情形仍调用module simplification和module optimization。

| 工作 | 当前完整Internal复制 | ThinLTO默认流程 | 可以声称的范围 |
| --- | --- | --- | --- |
| 前端Verify、ZIL折叠、完整machine预检 | 当前执行 | 当前研究仍执行 | 不应把省掉这些检查算收益 |
| early SROA/EarlyCSE/CFG清理 | 每份CGU当前body | canonical prelink做，postlink跳过该early块 | 官方有避免重复清理的明确依据，见1120–1181；不等于后续简化只做一次 |
| IPSCCP、InstCombine、global/inliner简化 | 完整可见的私有body上执行 | prelink和postlink都可能执行 | 仍有重复，且输入/权限/顺序有差异 |
| vectorization等后段优化 | 对所有未消除Internal实现/inline结果执行 | prelink延后，postlink执行 | 不宣称关闭向量器或降低O2 |
| 非inline导入body后段优化/emit | Internal body保留则仍可优化与emit | AE通常在postlink optimization前剥为声明 | 有机会省真实重复工作，但同时失去部分非inline IPO权限 |
| summary/thinlink/lazy import/bitcode I/O | 不需要此层 | 新增 | 必须计入完整请求成本 |
| native object缓存 | 现有逐unitcache | 当前quality probe用全新ThinLTO cache | 此轮无暖命中/增量编译提速证据 |

明确依据：[postlink跳early清理、仍IPSCCP:1120–1244](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1120-L1244)。prelink的IPSCCP禁function specialization，postlink/per-module允许（1224–1225）；这是一处管线差异，不表示后端能自动补回所有private-clone专门化。

[optimization管线1519–1534](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1519-L1534)在非prelink阶段先运行EliminateAvailableExternally，再进入后续函数/向量优化；因此未inline AE body并非都再做完整后段优化/机器emit。已inline到caller的指令仍会优化，这是保住跨leaf组合机会所必需的工作，不能为了提速删除。

[SLP与loop向量管线:1348–1492](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1348-L1492)仍存在；[lld配置:113–119](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/lld/ELF/LTO.cpp#L113-L119)在lto-O2明确打开LoopVectorization和SLPVectorization。原型启用了postlink merge-functions、禁止aliases/ICF；[1715–1721](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1715-L1721)证明MergeFunctions位于后段GlobalDCE/ConstantMerge之后。不能把不同代码笼统归因为“没跑完整O2”，也不能称ThinLTO管线与当前default<O2>逐pass等价。

## 四、为何完整导入仍可能缺旧范围证明

[FunctionImportUtils.cpp:180–190](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/Utils/FunctionImportUtils.cpp#L180-L190)通常把导入的External/LinkOnceODR主体变成available_externally（存在SymbolsToMove例外；必须读实际linkage，不能猜）。这保留规范符号而不重复定义，与ZIL当前`zil.import.<owner>.<canonical>`的Internal实现不是同一权限模型。

[GlobalValue.h:139–158](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/GlobalValue.h#L139-L158)规定AE允许derefine；[470–507](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/GlobalValue.h#L470-L507)解释非exact定义会限制非inline IPO，而inline仍可合法。于是[ValueLatticeUtils.cpp:19–25](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Analysis/ValueLatticeUtils.cpp#L19-L25)的参数跟踪（exact+local+无address-taken）及返回值跟踪（exact且非naked）都不能仅凭AE body可见就启用。[SCCP.cpp:130–146、275–276](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/SCCP.cpp#L130-L146)实际消费这些条件并推导返回/参数属性。

这给出一个明确但尚未实例化的风险链：AE未inline → 非exact跨函数返回/参数分析受限 → caller中的checked add/overflow edge无法证明无溢出；或未inline本身使跨leaf算术无法组合 → caller无原SIMD机会。两条链应分别找证据，不把所有jo或call增加都归为同一pass失败。已有prelink函数可能带返回range属性，owner也可能经thinlink internalize后恢复exact；必须比较实际owner、import、callsite的attrs与IR，不断言每个AE调用都会丢range。

[thinLTOFinalizeInModule:1712–1737](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L1712-L1737)传播readnone/readonly/norecurse/nounwind等summary属性，但这段不是任意整型返回范围的完整跨模块证明器。已有attrs若能保住某检查，并不等于恢复跨调用SIMD；反过来某个检查仍在也不证明最终runtime门必失败。

这与历史完整AE NoGo的机制相容，不能把ThinLTO当成自动绕开旧反例的证明。它的新依据只能是本轮真实prelink/summary/导入/internalization/优化结果与运行门，而不是换一个AE入口名称。

## 五、收到真实两case结果后的诊断顺序

### 先利用当前已保存证据，不新增采样

1. **绑定真实阶段与root。** 从完整prepartition sidecar/当前fold后输入确认A/B同一原始程序；从每unit input.ll确认canonical owner、root和leaf分布，B没有ZIL预复制Internal imports。按saved-files.json的original路径映射CGU，不按复制文件的排序号猜unit。
2. **读取prelink后规模和属性。** 对n64/n256每个关键leaf的value函数记录定义linkage、LLVM指令数、调用图、返回/参数attrs、剩余overflow intrinsics与trap边；对照summary的instCount。源码8个函数可能已经在本文件prelink合并/inline成一个大value，100门应作用于这个真实结果。
3. **区分模块来源与具体body。** 当前`.imports`只列被导入的源模块，不列每个具体定义；非空仅证明有来源，不证明全部value定义都被导入。检查`.3.import.bc`里的真实define及linkage，并与prelink的目标函数对上，区分声明、body、owner与导入副本。
4. **对照postlink，而非只数最终符号。** `.3.import`有AE定义、`.4.opt`没有该定义时，要查root中的相应call是否消失：消失可能inline/DCE；call仍在且目标变声明才是未inline body被剥除。记录root静态call目标/次数、vector IR、overflow intrinsic/分支与返回range；实际机器码再记录calls、SIMD、jo→trap路径与root/leaf大小。不能仅凭最终nm里有规范symbol就认定没有inline。
5. **结合运行门。** 若两case B/A≤1.10且R/B≥.90且所有功能输出通过，只说明此有限pure-base范围可继续讨论；若未过，以上证据用于定位导入缺失、inline选择、非inline范围分析或target/pipeline差异。任何结果都不改循环、O2、检查或原固定门。

官方save-temps阶段由[LTOBackend.cpp:169–189](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTOBackend.cpp#L169-L189)绑定：`0.preopt`、`1.promote`、`2.internalize`、`3.import`、`4.opt`、`5.precodegen`。其中`.3.import`在真实导入完成、postlink优化之前保存；`.4.opt`才是优化后，官方[652–785](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTOBackend.cpp#L652-L785)给出调用顺序。当前wrapper已保存这些bitcode，不应另开大量编译来取得同一证据。需要反汇编/llvm-dis时由Root在当前性能窗口结束后统一安排，命令及产物另记身份；本报告没有执行。

### 只有当前证据仍无法区分原因时，才考虑单次诊断

官方`FunctionImport.cpp:123–128、1119–1139`提供`print-imports`与`print-import-failures`，失败项含Reason、Threshold、Size、MaxHotness、Attempts。可在Root另行授权的诊断调用中透过lld `--mllvm`传递，不改导入阈值、hotness或force选项。它能区分真实导入拒绝，不保证显示后续inliner拒绝。

[lld Options.td:649、683–694](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/lld/ELF/Options.td#L649-L694)确认`--lto-debug-pass-manager`与`--opt-remarks-filename`、`--opt-remarks-passes`、`--opt-remarks-format`。后者的passes参数是regex，可只取inline/loop-vectorize/slp-vectorizer相关remarks；`LTOBackend.cpp:667–674`在每backend建立真实remarks输出。额外诊断会改变I/O/计时，必须新标诊断身份，不混入冻结质量或编译性能样本，也不因remarks缺失推断某pass没跑。

目前不建议任何阈值扫描、预inline、loop回退、手写range attrs或降低检查；这些会同时改变机制与质量，失去对本轮默认ThinLTO的可解释性。


## 六、随后补核：Blaze真实行为及n64已保存文本

### Blaze并未把一般import改成Internal

精确本地来源：`/workspace/scratch/abf690ce880c/research/blaze-source`，HEAD `ae1a7a7a1771c1658173ee4829392c40b4f1d4cc`，上游`indexable-inc/blaze`。该checkout为sparse；本次用`git show HEAD:<path>`只读缺省目录之外的精确Git对象，没有更改checkout或执行Blaze。主要blob为`llvm/release.rs`=`ef82c61fde01c1f19305d09f8aa3ce4dcdcc3911`，`llvm.rs`=`4edcdc605fe488218b12bd35144c6de5eb886e41`，codegen `imports.rs`=`501c8f3aab813741e4698a2fcad646179bb2294e`，`driver.rs`=`164785301cf1ba77048f6959ba17991598398f46`。

[release.rs:60–68](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/llvm/release.rs#L60-L68)的Prelinked保存own/export bitcode，export只是去debug的同一prelinked IR；[248–269](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/llvm/release.rs#L248-L269)先跑prelink pass，再统计真实instruction、读facts、写own/export。同一prepared IR也供own compile使用，不二次lower；[llvm.rs:709–715](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/llvm.rs#L709-L715)是SROA/instsimplify/jump-threading加thinlto-pre-link<O2/O3>。这确实是可复用的预优化export，**不是最终完整postlink O2机器产物，也不是每次导入原始body重跑prelink**。

但[release.rs:646–707](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/llvm/release.rs#L646-L707)明确以`Linkage::AvailableExternally`链接import，随后跑postlink（Default为thinlto<O2>），最后额外elim-avail-extern。`llvm.rs::link:779–807`在全部导入模块合并后对每个目标函数`set_linkage(linkage)`，release实际传入AE；没有把一般import internalize为私有clone的后续步骤。`llvm.rs:533`存在的Internal是MachO唯一内存wrapper的inner函数，不是这里的通用导入策略，不能拿它证明private-specialize机制。

### Canonical request映射与递归

[llvm.rs:814–909](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/llvm.rs#L814-L909)从export.bitcode读模块，把own改为调用方的`Symbol::Func(import.id)`，export里的Func/Data请求通过import.funcs/datas重映射为调用方请求。自递归声明若映到own_name，其所有使用先替换为own主体，避免两份自身符号；重复请求通过one_per_request合并，先临时命名再赋最终名并校验LLVM没有悄悄加后缀。

第905行临时External只是为了先把import modules成对link后一次接进目的module，紧接着link函数按AE设回；不能把这段临时linkage当最终权限。最终AE不发射第二个规范定义，函数/数据请求沿同一映射保留；readonly对象也用AE，不据此创造第二个地址。这里有一个真实工程去冗余点：导入模块先成对合并，避免每次link重复遍历越来越大的目的模块；但ZIL当前走结构化Module分配/Builder而非逐import LLVM module linker，不能直接称同一O(N²)瓶颈存在于ZIL。

以上说明Blaze canonical身份的静态安排，不等于已证明ZIL所有address/global/helper合同可照搬。Blaze导入层有自己的请求重映射与重复target合并，ZIL现有“地址留canonical、仅直调改私有clone”边界仍须独立保留。

### Private specialization来自何处：额外全程序facts，不是Internal import

[release.rs:767–881](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/llvm/release.rs#L767-L881)的apply接收Given：把已证明的constant参数直接RAUW为常量，给callee附加nounwind/willreturn/norecurse/noreturn/memory属性，并把证明dead的参数改poison且去掉相关UB属性。这是对**当前own主体和其调用事实**的独立全程序信息应用；没有创建一般import的Internal私有专门化副本。

来源[codegen/imports.rs:8–12、233–275、294–334](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/imports.rs#L294-L334)严格只给external=false且没有任何summary取地址、全部调用者可知的closed函数推导常量/dead参数；driver close提供代码外可达边界。所有调用点必须对常量一致。不能在ZIL里因为源码看起来纯或某次测试没有取地址，就自行附加这些事实。

[driver.rs:460–485](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/driver.rs#L460-L485)把导入export实际单位身份、refs及其MIR已inline的built_in依赖一起进入release key，防止只按callee表面key复用另一份export；[4170–4230](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/driver.rs#L4170-L4230)复用summaries留下的prepared IR与exports，缺失export由本build的Exports共享构建。预优化缓存的有效性不能省略这些传递依赖。

因此可借鉴的已证机制是“每函数prelink一次、复用own/export、summary与完整依赖绑定、独立closed-world facts”；**没有证据支持“Blaze已用Internal导入保住ZIL旧强IPO且省emit”这个说法**。它也保留100/300、0.7、depth8、budget40000的自有导入限制（imports.rs:29–54），不是默认LLVM ThinLTO全图复制；其仓库性能注释不作为ZIL实测证据。

### n64的现有文本已区分第一层原因

Root在两case正式质量探针仍执行时告知首个n64功能全对、B/A约1.7358。本人没有新增运行，也未读取/反编译`.3.import.bc`或`.4.opt.bc`；只读已落盘receipt及prelink.ll，证据提取在`n64-existing-text-audit.json`，保留原路径及SHA。

`out/next-optimization/thin-runtime-v1/n64/base/compile-B/wrapper/1794390984581631432-73/receipt.json`的八个actual_imports全部为空，cache_was_initially_empty=true。因此当前证据支持“这次链接没有实际导入来源模块”，**不支持把首败直接归为AE导入后未inline**。下一步可用保存的`.3.import`核具体函数，但不能倒置这两层。

预链接文本已含真实summary：63个leaf value定义`zil.fn.14,23,…572`的insts全部大于100，最小122、最大184；例如fn14=178，fn23=156，读取条目的notEligibleToImport/noInline/alwaysInline均为0。fn14返回range为[-2000,2001)，其单个step辅助函数常见range为[-250,251)；不能把step属性误记为整个leaf value属性。root `zil.fn.2`在prelink仍调用这63个value并有checked-add intrinsics。

这与官方普通100大小门相符，为未导入提供强机制解释；当前没有print-import-failures日志，故“所有拒绝Reason字段已逐项证为TooLarge”尚不能声称。没有导入body时，root无法获得跨leaf完整算术主体，旧SIMD组合机会存在直接风险；具体postlink/机器码及范围检查变化仍需实际产物确认。当前不调整100、不加alwaysinline、不扫描参数，不把Blaze自身有门的AE导入当作恢复质量保证。


## 七、正式质量窗口结束后的真实.3/.4解码

Root已完成冻结default ThinLTO两case：6 clean、54 native、36 formal，输出均正确；n64 B/A=1.7357655，n256=2.0076493，两项均越1.10，整体NoGo。本节只消费这些已保存产物，不另跑编译或native，不将解码耗时计入任何性能样本。

按Root授权，使用protocol pin的llvm-dis解码两case×八unit×import/opt两阶段，共32份原有bitcode，输出`decoded-default-v1/`。每个输入先按原saved receipt核SHA，解码后再次核输入SHA；`decoded-index.json`记录原staging路径、保存路径/SHA、输出路径/SHA及实际argv/exit。`ir-analysis.json`记录函数集合、call目标、typed-vector行与overflow intrinsics。这里的指令行数/typed-vector行是IR文本诊断，不是机器指令数、SIMD次数或运行计时。

| 已保存事实 | n64 | n256 |
| --- | --- | --- |
| imports非空的unit | 0/8 | 0/8 |
| .3定义中不属于自身prelink定义集合的新函数 | 全8unit为空 | 全8unit为空 |
| .3/.4 AE定义 | 全部0 | 全部0 |
| prelink leaf value summary规模 | 63个，122–184，全部>100 | 255个，119–184，全部>100 |
| .3 root的leaf直接调用 | 63 | 255 |
| 其中本owner已定义callee | 7 | 31 |
| .4 hot函数剩余leaf调用 | 56 | 224 |
| .4调用目标集是否恰好等于原跨owner目标集 | 是 | 是 |
| .4 hot函数身份 | main，root/源main已并入 | internal fastcc zil.fn.2，主循环在main |
| .4 hot函数IR指令行 | 741 | 2113 |
| .4 hot函数typed-vector行 | 154 | 225 |
| .4 hot函数sadd.with.overflow.i32 | 64（含主循环2） | 254；main另2 |

这比只读imports文件更强：.3没有新增任何跨模块函数定义，.4消除的leaf调用恰好为同owner的7/31个，保留下来的集合就是56/224个跨owner调用。故**这次默认ThinLTO未恢复跨unit body可见性**，不能把这两次实际退化归成“完整AE导入后仍不inline”。AE/exact-definition仍是下一对照的潜在风险，目前未在这个zero-import结果中被实例化。

向量化也需准确区分：postlink hot函数含154/225个typed-vector行，说明仍有局部向量IR；不能宣称整个vectorizer关闭或全部SIMD消失。它获得的body仅限同owner，缺的是其他owner的跨leaf组合机会。没有本节新跑的pass remarks或汇编统计，不给具体SLP pass或最终机器SIMD数量归因。原A的完整导入仍有更广可见性，两者最终热点形状也不同；总运行差异不能只按静态call数换算。

## 八、官方强制导入机制及严格研究对照可行性

### 准确选项与作用域

官方23.1.4确有隐藏bool [`force-import-all`](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L80-L88)，默认false。按现有wrapper已使用的lld LLVM-option转发形式，候选命令增量可写为：

```text
-Wl,--mllvm=-force-import-all=true
```

这里只确认官方声明和lld `--mllvm`转发契约，未执行此新flag、未作可运行/运行质量承诺。它是一次机制选择，不需把import-instr-limit增到任意大值，也不改evolution参数。

真实效果见[selectCallee:326–337](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L326-L337)：绕过instruction大小和noinline的导入拒绝，但**不会绕过前面的live、linkage、notEligible、local归属等合法性检查**，也不会清除callee本身的noinline属性。遍历仍是live模块函数的calls闭包；不是导入index的所有函数、地址引用或所有globals，不改变导入linkage为Internal，也不承诺最终inline。

它依旧受ImportCutoff、已有定义跳过以及特殊workload canImport等路径约束。更关键的是[1015–1023](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L1015-L1023)：当某callee不能选择时，force分支记录`Error importing module: Failed to import function ...`，随后break当前函数的calls遍历，并不是返回一个必然令lld非零退出的fatal Error。因此“链接exit0”不能证明完整强制导入。

合法C extern（getchar/putchar）没有程序内body/summary，也可能触发该分支；当前研究尚未运行force，不能先断言它们实际会打印什么。若未来协议接受“已知外部无body”的诊断，必须从当前原声明/prelink图证明该symbol没有owner/body且不属于Required，保留raw并限定外部集合；不得全局忽略导入错误。反过来要求所有LLVM stderr必空也可能把合法外部调用当成本体导入失败，主要证据应是精确覆盖验证。

另一个名字类似的[`import-all-index`](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L2146-L2180)只在opt的`doImportingForModuleForTest`测试路径消费，用于指定summary-file的distributed index实验，不是当前正常lld后端的“全导入”开关；不能随手传给lld便声称生效。

### Root提出的原planner allowlist对照：有条件可行

这个对照不同于阈值扫描：先由同一current machine上的原ResourceHelpers/Complete planner给出完整call可见性的**有限许可与原始费用**，再用预优化canonical bitcode加force一次，只验证在该许可内能否恢复质量。原default两caseNoGo与原raw永久保留；新对照须单独源码/脚本/protocol身份、完整原6/54/36和1.10/.90门。没有任何编译提速预先结论。

应具备以下充分可审条件；当前仍是设计评估，没有实现：

1. **同一输入与同一原policy。** 在当前original/fold后机器Module上走与A相同的ResourceHelpers/Complete实际规划，原地址/local-state/compiler-owned拒绝、cost/copyfuel和fallback均保持。纯base适用范围要在导出时实际核定，不能由fixture文件名替代。完整machine与各unit原预检继续执行。
2. **不能只看“8units、fallback=None”。** 当前machine_cgu.rs的partition_view_recorded会在Complete超总copy成本时转ordinary planner，并清理原recorded；后者完全可能仍给8units/fallbackNone。因此必须用实际`zil.import.{owner}.{canonical}`目标与原source-call闭包覆盖关系，证明得到的是完整且未超预算的许可。空名单不能因普通fallback而当完整成功。
3. **精确收费与owner映射。** 从原current canonical函数读取function.cost（instructions+blocks），按每unit额外复制目标逐项收费，sum≤原source_functions总cost；原planner的checked_add/拒绝不变。需要记录每项canonical/owner/originalcost及总和。Complete在split_large_files时可能启用small_caller_components，而Disabled不启用，所以必须比较每unit完整canonical owner定义集合；仅同为8units、按vector index配名单不够。同owner不成立就拒绝该对照，不为跑通重排。
4. **Required来自实际prelink图。** 本轮canonical Disabled导出后，按真实prelink各函数定义/直调边，计算每目的unit由自身live定义可达的跨owner canonical body闭包；intrinsic/程序外无body声明不算Required。必须Required⊆原Allowed，且所有Required目标的原身份/类型匹配。预链接可能消掉原调用，故无需把原Allowed里已无调用需求的每个step强行导入；但不能拿这个事实漏掉仍需的value。
5. **post-import既防超额也防缺失。** 检查所有实际跨owner/AE定义⊆原Allowed，并要求Required⊆`.3.import`实际definition集合；owner本地已有body可以满足相应依赖。只做AE⊆Allowed是subset安全，不是完整可见性证明。还应把任何新出现的非AE源函数定义/被重命名定义明确映射或拒绝，不能仅检查AE而漏掉另一类实际导入。
6. **费用和完整性的证据前后绑定。** allowlist/cost receipt必须绑定精确当前machine、source/compiler/tree、unit owner集合与prelink输入hash；wrapper不接受人工拼的名单或另一请求残留。force option本身不保护copy预算；在当前exporter仅Disabled且无witness的情况下直接加flag不符合这个对照。

这里的原machine费用证明保持的是ZIL原copy合同，不声称prelink/postlink膨胀后的LLVM instruction数仍等于费用。预优化body可能已经inline原callee；它包含的语义依赖仍应在原call闭包许可与缓存依赖里，不能只按优化后表面函数名忽略原built-in依赖。这也是Blaze with_imported额外纳入export实际built_in依赖的理由。

若所有条件成立，这个单一对照可以回答“在同8unit和原完整可见性费用内，复用prelink后恢复body导入能否挽回运行质量”。若仍失败，才有依据进一步区分AE非exact IPO、postlink inline成本/顺序及前置简化效果；不能用继续加阈值、绕身份限制、扩大费用、改循环或补手写range使它过门。Root已交另一作者设计新独立候选，本报告未改其源码或脚本。
