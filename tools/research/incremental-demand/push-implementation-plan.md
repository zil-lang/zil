# 全流程 push：可行性与代码施工边界

本轮为源码级可行性分析及既有对象文件的只读比较，没有生产源码修改、新编译或新性能测量。目标仍是显著领先 Rust，允许语言到 LLVM 重设计；保留安全、代码质量和明确的合同变更。

远端复核：main已从旧测量基线99035931前移到42f0396f36b2a36860ecb55371d436e4327877a4（新增整数/FFI等语言覆盖），goal读取81f7b43c981270a1572f490d5540a150358ba78e。上述代码写集以研究编译器900为依据；进入实施前须在独立分支对齐新main，补齐新增Type/Constant/ABI的身份、Wire及独立验证依赖，保留旧冻结证据并重建新对照。旧结果不是当前main的测量，未与其他施工分支作性能比较。

## 结论分级

| 方向 | 可行性结论 | 未闭合问题 |
| --- | --- | --- |
| 从变化输入驱动真实函数消费者 | 可实施的工程改造，已有图、函数结果和独立证明基础 | 目前外层整项目key/整表消费者仍在，不能只增加NodeKind |
| 逐项表示、精确汇总与位置解耦 | 可实施，但涉及公共IR及Wire/指纹合同迁移 | 稳定逻辑身份与稠密索引、首错顺序、预算、全局关系必须分别处理 |
| 独立验证的增量组合 | 已有局部证明基础，按义务逐类扩展 | 全局/跨函数义务不能自动视作函数局部，graph dirty位不授信 |
| 按需LLVM文本、共享raw编码片段 | 接口和合同明确后可实施 | 当前BuildArtifact/报告承诺LLVM路径；两Store字节头和预算不同 |
| O2后逐函数原生发射复用 | 有输出复用机会，但生产实现尚未证成，需LLVM原型 | 当前C API只发射整模块；模块级codegen、常量、重定位、展开/debug等边界未闭合 |
| 显著领先Rust的最终速度 | 工程目标，不是已证结论 | 必须用同窗口完整构建、负场景和运行质量验证 |

## 必须新增/完善的能力及写集

下列新类型/API名字是提案，不是已经存在的代码。

| 能力 | 现有入口/问题 | 具体修改 |
| --- | --- | --- |
| 变化事务与稳定身份 | driver/project.rs按project_fingerprint进入；Types有FunctionOwner但准备阶段仍汇总全部keys | 新增ChangeBatch/ItemId，分离body、合同、布局/ABI、位置、外部构建输入；用稳定owner寻址结果，版本不作为新owner。新增/删除/负名字查询必须进入变化集 |
| 实际产物调度 | db/reactive.rs已有batch、直接通知、结果相等停止，但当前仅用于BodyKey；失败poison后clear | 图承载实际产物句柄/投影，事务式发布deps/value/结果；增加active消费者、删除与失败回滚。有效递归应由显式合同或SCC求解器处理，不能让图的Cycle错误拒绝合法递归。避免把整张图clone作为新全局瓶颈 |
| 常驻逐项IR | ir::TypedModule.functions及CandidatePlan.functions仍为Vec，内部blocks/holds已共享 | 逐项持久化存储与只读视图；保留旧raw整表入口兼容并严格验证。逻辑ItemId与既有稠密FunctionId分层，逐消费者迁移；不能一边完整物化旧Vec一边宣称已消除整表工作 |
| 源位置与语义分离 | expand/incremental.rs、types/program.rs及incremental.rs仍有当前模块Lower/物化、span重绑 | 稳定语义项使用相对位置，SourceMap绑定当前文件/编辑；位置变动只通知读位置的消费者。声明/环境/CT变化仍更新实际语义依赖 |
| Types真实节点 | Reuse::prepare生成完整FunctionFingerprints表，再由整程序检查消费 | 提取当前函数CFG执行入口和ContextView依赖读取；变化项直接调用，结果存入持久化集合。完整接口/形状检查分成可复用义务，未知边界保守回退 |
| Analyze真实节点 | analyze_budgeted先preflight，再CandidatePlan::empty_for整模块fingerprint，再for所有functions | 拆逐函数候选、公共合同、全局约束节点；用变更更新计数/特征/索引/有序摘要，替换全量编码指纹。预算与诊断优先序不可只用一个总和近似 |
| 独立Verify节点 | VerifySession已有FlowReceipt等；verify_shared_program_module仍处理整模块未分解义务 | verifier私有输入/依赖/证明存储，局部证明加显式全局义务；完整当前证明集合才能产生当前模块权限。Analyze值、缓存摘要和dirty标记均不能构造权限 |
| 后端变化消费 | emit/incremental已有函数机器产物；BackendSession仍接收完整Module，render仍for全部items | 引入MachineDelta及稳定索引，更新实际CGU成员/动作输入，保留未变产物。传递完整当前Verify/LIR授权。代码身份与位置/显示名称需明确分层 |
| 按需产物 | build.rs总是生成/写出LLVM文本；BuildArtifact.llvm_ir为PathBuf，driver总将llvm放入报告 | 新增ArtifactDemand，LLVM路径/延迟物化为显式接口；同步修改dump、报告、发布/别名保护和消费者测试。仅在新合同成立后，未请求的文本产物才不计算 |
| 共享编码 | types/incremental_items.rs及driver/semantic_items.rs有两套函数体编码 | 由私有不可变owner提供共同body片段，分别组装各自header/name；保留各自wire字节、aggregate限额、原子发布及错误行为 |
| 构建输入与link freshness | backend/link_cache.rs::key仍发现工具/库/目录；命中不等于零探测 | 可复用的ResolvedLinkInputs与真实外部输入节点；冻结工具链快照需可证明，普通可变目录继续发现。不可把外部变化漏出依赖图或移出宣称的计时边界 |

## 全局检查和预算不能靠一个树哈希解决

持久化摘要可减少更新计数、索引和有序集合的遍历，但不能自动增量化所有关系型检查。每项全局义务都需说明读什么、由哪些变化失效。Merkle摘要若替代当前完整Wire流指纹，属于版本化合同改变，不能假装两者字节身份相同。

预算需要当前逻辑收费、确定性的检查顺序与失败位置。可组合收费/诊断索引要逐类证明；不能因为CPU未重算就少收费，也不能因工作队列调度顺序改变原首错。复杂失败可先回到原检查路径，但必须明确回退计数及性能负结果。失败/取消不覆盖上一成功epoch，恢复继续使用完整正确状态。

## LLVM原型：先验证复用边界，再承诺实现

当前zil-llvm-api/src/native.rs流程是：build完整LLVM Module → verify → LLVMRunPasses(default<O2>) → verify → LLVMTargetMachineEmitToMemoryBuffer整模块。函数退出即释放这些LLVM所有者；不存在现成的逐函数机器码缓存接口。

最小预研步骤：

1. 分离 optimize_unit 与 emit_unit 生命周期，保留Context/Module/TargetMachine的合法所有权；第一步仍调用原整模块emit，建立字节/错误等价基线，不声称已加速。
2. 提取优化后函数、属性、ABI及真实全局引用，确定哪些模块级codegen输入必须绑定。不能只比较函数文本，也不能把其它函数删成声明后假设上下文不变。
3. 用固定LLVM提交的C++桥接/扩展建立可重放原生产物边界，记录机器码、符号及重定位贡献；处理模块级pass、常量/局部符号、地址身份、COMDAT、展开/debug。现有C API不直接提供该能力，是否必须修改LLVM backend需由原型决定。
4. 与同输入的原完整发射比较完整产物、链接、执行、代码大小与运行质量。未证明域仍完整发射。第一阶段即使成功，也仍重做必要的O2区域优化；不冒充LLVM全流程逐函数更新。

## 已有对象的机会检查

只读比较正式900矩阵block0候选cold/private的CGU2：42个已定义FUNC符号中41个的正文机器码、所选ELF元数据及函数相对符号重定位记录相同，仅zil.fn.5不同；冷函数正文49504字节中48315字节来自这些未变函数。输入.o已经保存在固定archive32e1b04fb的v4中。

工具与结果见inspect_native_function_reuse.py及native-function-reuse-900.json；非作者使用GNU readelf/objdump独立复核，收据为native-function-reuse-900-review.json。此比较不覆盖全局常量内容、unwind/debug/COMDAT或LLVM上下文独立性；不是可安全拼接/跳过codegen的证明，更不是新的提速测量。它把后端构想从纯猜测推进到有明确输出复用机会的待证原型。

## 第一条施工链及退出条件

首个实现切片使用原425函数真实项目和当前合同，不先改语言或等待LLVM：ChangeBatch → CFG结果 → Analyze候选 → 独立Verify证明 → MachineDelta → 当前CGU/object/link。其目的是真正替换外层整项目失效和中间整表消费，既有局部产品直接作为节点结果使用，不重写已经有效的SharedVec共享。

补充计数：实际调度节点/依赖边、无关函数被访问次数、整项目物化次数、完整指纹字节、真实编码/写出字节；不得把这些隐藏在准备阶段。未变函数不再为查缓存而逐项进入。函数新增/删除/改名、合同变化、仅位置变化、递归、导入/CT变化、错误/取消/恢复、预算N/N−1都要与fresh路径核对；尚未覆盖的域明确回退。

LLVM发射原型作为另一条有明确成功/失败门槛的研发线。保持原425函数矩阵并增加困难实际场景，不只选择m0；检查与原生分开，计时到完整可执行文件及当前承诺的发布边界，保留冷/广负结果。先证明少做了真实工作，再用同窗口main/Rust对照判断是否足以显著领先。

产物合同改变时保留原完整产物模式对照，新增exe-only需求模式另行报告；不能少交付已请求的产物，再宣称是相同输出条件的速度提升。
