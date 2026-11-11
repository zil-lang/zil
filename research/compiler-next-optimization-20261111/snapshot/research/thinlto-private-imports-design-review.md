# ThinLTO预链接body转私有Internal：独立API与语义设计审查

## 结论与范围

**LLVM23.1.4官方API支持所提插入点；在当前严格pure、完整预算许可、全部uses均为精确CallInst callee、其他unit canonical owner不变的限定范围内，原地rename+Internal具有可解释的语义基础，未发现设计不可行点。** 这不是尚未冻结C++的精确源码验收，也不是编译/加载/运行质量或收益证明。需要后续按exact plugin bytes、wrapper/protocol和真实两case独审/执行。

本审只读官方pin源码、既有正式/诊断IR和工具ELF元数据，未编译、加载或执行LLVM plugin/编译器/native。采用Root最新限定：只setName/setLinkage，不增加零use旧canonical declaration。任何扩大到函数地址、全局状态、helper或未知use都不由本设计授权。

## 一、官方加载链与准确hook

固定LLVM源commit为`7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`（llvmorg-23.1.4）。

| 链路 | 精确依据 |
| --- | --- |
| lld命令接受插件 | [lld/ELF/Options.td:848](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/lld/ELF/Options.td#L848)定义`--load-pass-plugin`；Driver.cpp:1857收集，LTO.cpp:136–137装入Config.PassPluginFilenames |
| 每个backend构造PassBuilder并注册插件 | [llvm/lib/LTO/LTOBackend.cpp:202–218、297–299](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTOBackend.cpp#L202-L218)调用PassPlugin::Load和registerPassBuilderCallbacks |
| API/头路径 | [`llvm/Plugins/PassPlugin.h`](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/Plugins/PassPlugin.h#L29-L63)，不是旧`llvm/Passes/PassPlugin.h`；LLVM_PLUGIN_API_VERSION为2，导出`llvmGetPassPluginInfo` |
| callback真实签名 | [PassBuilder.h:506–513](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/Passes/PassBuilder.h#L506-L513)：`registerPipelineEarlySimplificationEPCallback(ModulePassManager&, OptimizationLevel, ThinOrFullLTOPhase)` |
| 真正插入位置 | [PassBuilderPipelines.cpp:1218–1225](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1218-L1225)，在IPSCCP前；标准AlwaysInliner/CGSCC inliner位于1323–1328 |

必须按enum精确筛`ThinOrFullLTOPhase::ThinLTOPostLink`，并保持O2。该callback也可在prelink或普通module简化被组装，不能只凭callback名猜阶段。`registerPipelineStartEPCallback`的头文件注释明确不适用于LTO/ThinLTO backend，不能换成它。

实际import在LTOBackend.cpp:772–783完成并保存`.3.import`，之后才进入opt/PB管线；postlink core调用buildModuleSimplificationPipeline。因此此hook可在真实AE body已存在后、标准IPSCCP及inliner前恢复private linkage。它并非所有后端pass的绝对第一步：PGO ICP、OpenMPOpt、可选Attributor和DropTypeTests可能已在它之前执行。当前pure/无PGO范围应继续保持，不据“early”声称没有先行转换。

API2结构还有PreCodeGenCallback，它可替代正常codegen；本方案只注册PB扩展，其他callback必须nullptr，不用它绕默认管线。PassPlugin::Load仅核API版本，不核同LLVM commit/全部C++ABI；同pin headers、构建设置与SO字节仍须单独绑定。

只读本轮ld.lld ELF元数据发现它静态含LLVM（NEEDED无libLLVM），但动态符号表导出19181个LLVM symbols，具备宿主解析plugin符号的基础。不能据此代报加载成功。应避免把另一整份LLVM静态库塞进SO或意外引入第二个LLVM运行时/选项注册状态；实际构建、依赖和一次加载pilot另由Root安排，本审未运行。

## 二、为何当前pure范围不需要新增canonical声明

原AE定义在本backend中只是规范callee的一份优化用body，真正canonical owner仍在其他unit。若其**每个实际Use**都是本次模块内的直接CallInst callee，原地重命名这个Function对象后，全部原uses自动指向同一私有Function；setLinkage(Internal)只改变本module该实现的链接可见性，不修改其他module owner。该module中已经没有任何引用需要旧canonical名字，增加零use declaration并非语义必需。

这也保留递归/互递归的直接边：它们持有Function对象，而不是靠原名字字符串重解析。批量rename前先检查全部许可、目标、use和碰撞，再变更，避免部分变更后才发现不合法输入。由其他module发出的规范调用/地址仍解析到未改owner；本module若仍有规范地址需求，就不满足这个简化的前提，必须拒绝，而不能把地址也改成私有实现。

它与当前主线完整Internal importer的语义核心相同：实现可私有化、规范身份另有owner、仅直调改去实现。当前主线还主动保留canonical declaration供地址使用；本对照只在已经证明无这些用途的pure子集省去无用声明。不能把这个简化扩大成通用替代。

## 三、only-direct必须逐Use证明

最关键反例是`call @f(ptr @f)`：同一个CallInst同时把f作为callee和参数。仅检查`dyn_cast<CallBase>(User)`、`getCalledFunction()==F`会在两个Use上都通过，从而把地址身份误转私有。

官方[InstrTypes.h:1411–1433](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/InstrTypes.h#L1411-L1433)提供`getCalledOperandUse`及`isCallee(const Use*)`。应遍历`Use &U : F.uses()`，要求User为CallInst，且`CI->isCallee(&U)`、called operand就是F。Root限定CallInst比接受全部CallBase更保守，当前IR无invoke/callbr，不需扩大。

ConstantExpr cast/Gep、Array initializer、GlobalVariable、GlobalAlias/IFunc、llvm.used、store/return/phi、把f作为任意call参数、未知user全部拒绝。不能stripPointerCasts后把非原始Global直调放行，不能用users集合或references减direct_calls替代逐Use。无普通Use不证明任意模块string/metadata都无名字意义，因此scope仍要排除module asm、未知address/type-test/CFI/PGO等路径；当前pure固定输入和完整shape校验是其边界，不给一般LLVM模块授权。

## 四、名单、owner与变更的必要不变量

1. 每个被改Function必须是当前实际`.3`导入的AE definition，canonical名字在本unit原预算Allowed内、属于真实Required，且不在本unitCanonicalOwn。原unit owner不能因“名字也在Allowed文件”被重命名。原Rust planner witness、独立Python图/成本、实际.3 visibility三层保持；plugin不能把环境字符串本身当原预算权威。
2. 当前实际导入没有越过原plan费用，plugin不能新增body导入或扩大名单。原function.cost来源与预算上限保持；更换linkage不改变原始费用计量，也不声称预链接优化后的IR体积等于费用。
3. 私有名字必须由当前unit与canonical名确定，形成一一映射；先用Module.getNamedValue检查包括函数/global/alias的冲突，任何已占用拒绝。LLVM setName可能自动加后缀，变更后必须核实际名字精确等于预期，不接受自动消歧；重复pass/同unit重复receipt也应拒绝。
4. 只setName/setLinkage，不创建alias、不加alwaysinline、不调cost阈值、不变calling convention、FunctionType、参数/返回attrs、instruction或block顺序。当前root要做before/after全module文本逆names/linkage归一化相等，这可比“计数一样”更强；逆归一化必须准确处理符号token和header，不做任意子串替换而误消真实差异。
5. [GlobalValue.h:547–554](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/GlobalValue.h#L547-L554)规定setLinkage(Internal)会把visibility和DLL storage改默认，并设置隐含dso_local。它们是setter真实变化，需记录到receipt/归一化条件；不能事后copy旧attributes把它们改回invalid状态。当前pure可拒绝任何非default visibility/DLL、COMDAT及特殊section，避免顺手支持另一语义范围。
6. 不提升canonical或私有函数到global unnamed_addr，不用unnamed_addr/alias/ICF来创造地址等价。原有local_unnamed_addr等状态不能借这次更改扩大；现scope无地址使用，仍保留未修改owner的身份。
7. COMDAT尤其不能随意复制或清除：已验证AE的isDeclarationForLinker路径本就不允许COMDAT（[Verifier.cpp:655–656](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/IR/Verifier.cpp#L655-L656)）。当前遇到非预期COMDAT应拒绝，不引入weak/ODR兼容分支。

Function的非语义GUID/debug metadata不能拿来替代实际canonical名字/owner绑定；当前!guid为ThinLTO记录，变更前后除了批准的header/name之外其他内容保持。未来加入PGO/CFI/type metadata之前需另审，不能推断本次无地址证明自动覆盖它们。

## 五、验证、分析失效和并行receipt

新pass应在变更前确认当前module有效，变更后调用verifyModule，失败必须令link非零/拒绝而非只在stderr警告。返回PreservedAnalyses::none或准确失效集合，不能在改名/linkage之后声称全部analysis保留。原LTO before/after verifier、wrapper对真实.3/.4 verify以及当前ZIL Verify/machine preflight都继续执行，不互相替代。

每backend都有独立Module/Context/PassBuilder；只有不可变字符串/名单/费用配置可以跨线程共享，不能共享Function/Type/Module句柄。插件加载/注册可能在多个backend调用，注册回调不能推进一个全局owner计数器来推断module归属。应按真实link输入ModuleIdentifier与immutable witness精确匹配unit，不能用SourceFileName（所有unit可能同名）、线程号或处理顺序分配许可。

Thinlink可能在hook前drop dead definitions，按“当前完整canonical定义集合必须原样齐全”直接定位可能误拒；必须有明确的module-input/path/hash映射，再核当前剩余定义确属于该owner、Required/Allowed来自同一已冻结输入。任何无法唯一定位都拒绝，不猜unit。

每个实际backend独立写唯一receipt，包含phase、module/unit身份、配置与plugin SHA绑定、变更前AE→私有映射、逐Use数量、linkage/visibility/DLL/dso_local、变更前后文本/验证结果。只读共享config，独立atomic/exclusive文件；不用多线程append同一JSON或stderr逐行充当机器收据。即使无imports的7个unit也须写no-op receipt，外部要求exact8、每unit一次、无缺格/异常；变更中失败不得留下被当作Pass的完整收据。

## 六、cache边界不能遗漏plugin身份

官方[llvm/lib/LTO/LTO.cpp:138–207](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTO.cpp#L138-L207)的computeLTOCacheKey包括LLVM版本、MllvmArgs、模块/import/export等，但未包括PassPluginFilenames、SO字节或环境配置名单。不能把默认ThinLTO的cache key当plugin/config已绑定的证明。

本次研究继续每个clean编译使用全新空cache，要求8个hook receipt齐全；若cache直接命中跳过pass，就必须拒绝该次证据。未来要复用才另加plugin+完整policy域隔离与失效设计，当前不引入，也不据本轮谈warm性能。插件SO、exact构建命令/headers/config文件与hash必须进入protocol与raw身份，loader只核API2不足以保证同pin实现。

## 七、与旧CompleteInternal究竟区别在哪里

这次恢复的Internal语义和sole-call成本奖励不是新算法：旧完整Internal及当前主线已在使用，旧四臂也已比较过其质量。新固定对照的不同点在**工作阶段**：

| 阶段 | 当前/旧CompleteInternal | 拟研究组合 |
| --- | --- | --- |
| 分区/预算 | 原machine完整closure、原费用 | 相同原plan许可与费用，不扩 |
| LLVM输入 | 每unit带原机器body私有副本，构造后完整default O2 | canonical CGU先prelink一次，导入已有预优化bitcode |
| caller优化时实现linkage | Internal | 先真实AE import，hook恢复私有Internal |
| 重复工作 | 复制体仍参与当前构造/早期简化/O2，未inline可emit | 预链接简化可复用；import解析/克隆、postlink simplification/O2及可能私有emit仍存在 |
| 新风险 | 既有原始body顺序与优化结果 | 早期CGU prelink已把value45条膨胀为122–184条，恢复Internal不能回滚所有phase-ordering差异 |

它有来自本轮真实remarks的新机制依据：AE value确已导入仍TooCostly；同body规模的local唯一活用有15000成本奖励。这个组合可隔离“预链接复用+恢复已有私有优化权限”是否保住质量，区别于盲目重试AE或调阈值。但插件也新增加载、验证、receipt、bitcode和LLVM框架成本，当前没有完整请求净收益证据；即使两case质量恢复，也仅允许讨论后续正式编译门，不代表采用。

还不能断言15000奖励是所有回退唯一因素，或把565/562读成只差3；LLVM可能提前停止cost分析。原typed-vector/range/overflow、own/AE use及完整质量结果应继续保留。所有新比较仍用原两case、相同2CPU/8units、6clean/54native/36formal及B/A≤1.10、R/B≥.90；不按loop、阈值、attrs或预算继续找过门样本。

## 待精确源码审查

C++/SO与hook_checks.py冻结后，非作者须核实际phase分支、逐Use检查、碰撞、两个setter及其隐含字段、before/after验证/归一化、unit定位、异常和并行收据。当前报告只是设计可行性和条件列表，不把这些尚未读到的实现当作已完成。root负责唯一重窗口，作者/本审均不能据这份文档提前扩测或宣称性能结果。
