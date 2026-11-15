# 提前CGU复用：当前API安全边界与最小施工建议

2026-11-16，Astra只读审核，针对root当前backend链；未编辑仓库、Git、构建、native或测试。行号为本次读取的`/workspace/zil-push-main`，施工树可能继续变化。不要把本报告作为新实现已通过的收据。

**建议本批先完成session内“完整变换结果→受影响unit构造→原完整unit检查/裁剪复用→现有对象/最终产物路径”。不要把ClosedBody当作Source→CGU的授权，也不要先实现可从磁盘导入的MachineOutput权限。全局class仍可先完整计算，然后按实际变换后输出变化缩小partition工作；这是安全的中间实现，收益只归partition桶，不能声称已省掉整图transform。**

## 1. 各层实际权限不能混用

- `emit.rs:219–243`先执行当前metadata预约、`bind_lir`，再尝试reuse_machine。`emit/incremental/whole.rs:168–226`的命中同时比较完整module域、functions、layouts、options、limits及当前prefix ledger，重扣既有spent，只允许checked scalar domain。这是**进程内先前真实生产所绑定的映射**，不是看fingerprint。
- `emit/incremental.rs:532–548`函数复用仍需same_input/facts/collected且调用`reserve_current`；`550–570`重lower后完整FunctionBatch相等才复用output identity，source projection identity另外比较。已有“结果相等停止传播”的锚点，不应由新summary绕过。
- `incremental.rs:243–271`的ScalarIdentity/TransformProduct是进程内private immutable owner，不能从磁盘mint Arc身份。它只在当前prepare与实际函数产品建立后有效。`build.rs:425–484`完整identity相等可直接用旧transformed machine。
- `zil-llvm-api/src/preflight_receipt.rs:1–18,59–78`的PreflightReceipt**构造私有、无serialization/import**，只证完整Module结构检查；不是ZIL Verify、LLVM Verify或产物来源证明。完整triple/items相等可由checker返回新的当前receipt；hash/opaque“checked=true”不能代替。
- `build.rs:1644–1660`对象命中后仍`validate_module_with_receipt(current module, prior)`。`object_cache.rs:399–424`验证真实当前action key、长度、checksum；key按当前Module的全部items和namespace得到，不能用磁盘自报key代替当前计算/可信session映射。
- `final_product.rs:115–129,423–459`先比当前配置/profile和全部unit值，再逐unit调用checker授权；`build.rs:930–978`还做取消、publication/current profile等检查。其“before_codegen”是省对象物化/链接，当前仍在机器构造和partition之后，不能误称已经pre-machine。

**ClosedBody/WorldBound特别结论**：`native/cohort_products.h:14–16`明确要求当前closed domain/input/analyses/pipeline/config/owner输出oracles重新成立。`native/work_products.cpp:1774–1810`第一次实际FPM后比较完整pre/post World，才能seal ClosedBody并丢弃World；`1025–1078`区分closed与world匹配。`machine_products.cpp`相应绑定的是已存在LLVM/MachineFunction阶段输入。它们均发生在源lowering之后，不能证明某个raw Module是当前LIR的正确lowering。`NativeWorkProducts`仅resident opaque Arc（`lib.rs:315–325`），无可直接拿来恢复跨进程Source映射的公开API。本窗闭包native products也不应假定必有命中。

## 2. 最小安全session方案（推荐root先做这一整块）

### A. 完整变换后相等截断

保留`lower_output_with_session`及其bind/预算原序。当前整图checked-add/scalar/class仍执行；其结果与上次**变换后**Module在triple/items上完整相等时，且partition所有额外读依赖相同，复用已完成partition。

注意`PartitionInput::eq`当前scalar分支比较lower前产品身份（`incremental.rs:354–368`），原body改但变换结果相同不会命中。可在现有PartitionProduct增加已完成的实际transformed output引用/对应比较入口；不要修改ScalarIdentity定义让不同raw输入冒充同一身份。额外依赖仍包括options、稳定names/source grouping、collected、closures和完整源拓扑/ABI/预算读集合。`same_recipe_environment:346–351`是现有保守入口，但依赖context Arc；emit owner正缩小context时必须确认新context仍涵盖partition读取的所有字段，否则不能直接沿用旧“context相同就足够”推论。

这条路径只用已由当前执行得到的变换后实际值，因此不需要预测class代表变化，也不需要新的权限类型。

### B. 变换后部分变化：扩展现有recipe，不直接patch已pruned旧units

`machine_cgu/recipe.rs:15–21,90–98`现在显式拒绝imports，并按原layout位置索引definitions；不可以删掉两个guard就用于完整导入。`PartitionProduct.units`已经完成内部化和prune（`build.rs:691–739`），删除过定义，**不是旧layout的可替换模板**。

需要同批实现：
1. 在现有recipe中保留未裁剪的分区模板及source item→所有canonical/import occurrence的消费者关系；import occurrence必须保存owner-specific重命名映射，包括其内部callee与全局/声明依赖，不能把canonical body直接塞到`zil.import.owner.*`槽。
2. 当前变换后changed_items用完整值比较。只有成员集合、排序、ABI、linkage、引用/直接调用图、成本限制、导入策略等全部相关投影不变才复用拓扑；拓扑变更回完整planner。原`same_function_partition_inputs:132–147`的cost/references/direct_calls是下界而非完整imports域证明。`layout.valid:225–290`当前进口范围/重复/成本检查仍需要等价覆盖。
3. 产生完整当前未裁unit后，当前全unit内部化/closure linkage仍必须正确决定。若优化内部化重算，也需完整跨unit符号使用/地址根关系，而非“该unit body未改”就沿用旧linkage。
4. 原始unit完整值+prune policy与上次完全相等时，可复用private session中记录的“原始输入→已完成pruned输出”。必须在第一次完整prune成功后记入，失败/取消不记；原完整体错误不能因不可达被隐藏。原来的`closed_prune_removed`计数本身不是输入映射。改变的unit仍走`machine_closed_prune::prune`全部原输入检查及完整裁剪。
5. 然后保持`final_product::authorize`和`compile_api_unit_with_cohorts`原调用。未改unit的SharedVec可保留COW owner，checker当前匹配sealed receipt；变动unit fresh检查/生产。16个真实unit保留，telemetry区分当前fresh检查与既有完整输入reuse，不借用首次生产计数充当本次执行。

这可先省重复partition/clone/pre-prune工作，无需先做global class增量。若仍完整生成所有unpruned units后才比较，只能省prune及后续部分，不能宣称避免构造；应诚实分别计时。

## 3. 真正“global to_module之前”只在哪些条件下安全

整个ScalarIdentity保持时当前实现已能避开变换的to_module。对于修改请求，要在global to_module之前选某unit，必须拥有并比较该unit的**完整生产消费依赖**。现有函数lower输出能提供本地源绑定，但`TransformProduct`只有全图mapping，并无每函数class依赖receipt。因此现在没有一个现成API能仅凭“这一函数原body没改”授权它最终所在unit。

class依赖尤其不是只反向callee：最早代表函数修改/删除会让整个class换代表；新/删address-use会改变其他函数是否可归并；一函数进入/离开纯域、递归SCC或全局protected集合变化会向上影响；相等class被拆/合后所有原成员的调用目标都要更新。完整方案要跟踪class成员、代表排序、反向依赖、地址/入口保护、SCC/eligible变化及当前全图完整检查的等价义务。此刻如果root优先完整transform后partition值比较，是合理安全取舍，不应为了宣称Rust式early CGU而跳过这些依赖。

Rust58cf对应`codegen_ssa/base.rs:785,1238–1279`先要求work product存在，再让完整CompileCodegenUnit dep-node green；不是以“原函数没改”直接拿对象。ZIL可借顺序，不能借Rust算法名称冒充已经建立了依赖边。

## 4. 跨进程rawmachine恢复：当前LIR Eq是否足够？

**只比较磁盘record里的LIR与当前raw LIR，外加raw Module通过preflight，仍不够建立这个Module是该LIR的lowering。** 反例：保留正确输入LIR，换成结构合法但把返回常量改为别值的Module；独立Verify验证的是当前源/Typed，preflight只验证机器结构，两者都会各自成功，却缺Source→Machine这条对应关系。自身checksum/来源字段/自报action key不能补这个逻辑缺口。

区别：session内record由同一运行producer私有构造，输入/输出不可变配对保留，完整当前输入相等可以复用该映射；磁盘上的raw配对不能自动取得这种私有owner权限。现有`PreflightReceipt`、ScalarIdentity、NativeWorkProducts均没有安全的disk import接口；typed/Wire functionproducts也不证lowering后的机器语义。`machine_cgu/layout.rs:373–400`只把disk layout作为raw候选，最终`valid(current module,current LIR,count)`验证；它依赖当前machine，不能当作避免machine的证据。当前model未发现完整通用raw machine codec，勿假定现成decode即可复用。

若要真正跨进程提前路径，至少新增以下**实现义务而非新证书格式**之一：
- 在已有lowerer上提供可验证的窄域重放/对应检查，把current完整LIR及全部lower配置/预算与缓存FunctionBatch逐项绑定，未知域fallback；建立current输入→输出关系后才重建private session产品。必须比较所有输出字段及生成helpers/入口，不只是签名和控制流。它的成本可能接近原lower，先有成本证据才值得做。
- 或明确引入普通原始生产缓存的持久化信任边界：compiler/schema/config/input namespace、受控原子producer写入和当前完整性/损坏规则，承认新增source→machine memoized producer contract并按该边界审计。**不能声称这已由ClosedBody/原preflight授权覆盖，也不能把它包装成无需审核的现有权限。** 在用户“不新增serializedcertauthority”要求下，保守本批不走这条未闭合路径。

两者仍保留当前独立Verify、当前精确bind_lir、limits/prefix账本、first-production全部检查、后端配置/LLVM身份/entry/导入/链接profile绑定与取消。跨进程恢复暂未闭合就保留正确重建，单独报告重启缺口；不要默认开启一个只有checksum的rawmachine导入。

## 5. 同批必需的反例（root一次集中验证，无需本sidecar运行）

正确源LIR配错误但结构合法machine必须失败/不命中；相同诊断fingerprint不能替代输入；同body改callee ABI/负lookup/地址根必须失效；代表删除/拆分/合并、跨unit链接变化覆盖所有imports；未用非法机器体不可因prune漏验；缓存输入相等而policy/limits/target/entry/toolchain不同不得命中；cancel/失败后不能留下partial recipe或key_partition；imports replay与完整fresh planner的原未裁unit、最终unit和实际原生结果对照；当前preflight复用计数与首次生产LLVM前后Verify分开。

**施工优先级裁决：session内实际变换结果+完整imports recipe/原未裁unit映射是当前最小可落地链。先完整class、后按actual values缩小partition安全；pre-to_module class局部判定及diskrawmachine都不能仅删guard实现。**
