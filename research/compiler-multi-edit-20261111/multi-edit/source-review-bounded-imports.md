# 普通多文件有限导入候选独立源码审核

结论：**精确发布候选1679的源码审核通过，未发现阻断当前生产适用范围的正确性问题。** 本审核只读源码、测试和Git对象，没有构建、测试或采样；不把源码通过写成运行质量、性能或生产采用通过。

## 身份与范围

| 项目 | 精确身份 |
|---|---|
| 发布提交 | `1679decf177c9a6eda5a64063c525f06e44e56a2` |
| 发布tree | `1e599b1bc2f0f6a0c438088a30faee9ff4018271` |
| 发布父/main基线 | `99035931d358add0eaed6d4535035c38413701dd` |
| 同tree作者提交 | `1d29028c637b4af9a880a3db6eb8d678c172f740` |
| 生产resource_helpers.rs blob | `84456d4da4458d67d966048f43b56903c952453b` |
| tests.rs blob | `a26382526539a550d04577a20d6d5392be3d5271` |

独立GitHub API读取了发布SHA、父、tree、四个变更blob；本地干净checkout与其tree/生产及测试blob一致。相对main只改resource_helpers.rs、其tests.rs、build.md及resource-helper-partition.md；最后一个本地提交仅统一文档首段，生产及测试blob未变。layout.rs与main逐字一致，没有a60投影/流式key改动混入。

提交：https://github.com/zil-lang/zil/commit/1679decf177c9a6eda5a64063c525f06e44e56a2

## 生产delta为什么成立

生产改动只有两处准入放宽和一处等价快退：不再要求lir.closures非空、不再要求helpers中至少一个true；非foreign function文件集的BTreeSet分配换成迭代取first_file并检查是否出现其他file。无函数/单文件仍在分配planner集合之前返回None，2..64 unit与65536 item上限原样。

调用点仍只在CallImportPolicy::ResourceHelpers时进入，而该policy仍仅由显式incremental-o2且function-merging=after产生。default off、before-and-after、未启用incremental-o2以及单文件均保留原路径。没有按fixture名称、N/k、输入值或已有成绩选择策略。

当helpers全false时，members收集所有definition，helpers复制列表为空；后续算法无需依赖存在helper。每个canonical function/global只归属一个owner，同file的source definitions仍union，local symbol与所有真实引用者仍union，故内部地址、局部全局状态及其用户保持唯一且共址。entry与main wrapper、main与PE wrapper的原并组逻辑原样。

跨owner只在真实direct-call、target为source function、cost<=32且完整linkage==External时导入私有实现；大source保持canonical body与外部调用声明。地址operand、initializer和非直接调用不改指向clone；rewrite_direct_calls只重写直接callee。导入small source若继续引用local身份，会依原owners检查/union保持有效，跨owner不能安全表达则返回None。compiler-owned小definition不冒充source；clone符号冲突仍拒绝。

删除准入条件没有删除复制上限：extra checked_add、extra<=original_cost、百万fuel、source name/.lc1解析、重复source ID和machine符号拒绝、同参数分派共址均保留。single/members<2、原source缺失、超预算或其他拒绝仍保留完整输入、回到原ordinary planner，不部分提交。

最关键的坏model边界原样：prepare可能过滤unused declarations/死private helper，故只有末尾对**完整未改变输入**validate_module成功才返回Some。无helper并没有省略此检查；replay后原每unit当前preflight、语言独立Verify及LLVM前后核验链均未改。新准入改变了优化可见性和生成代码，不能拿先前a60的跨arm严格同码协议替代新的运行质量验收。

## 测试实读

新增4个test共293行，使用清空closures后的三source fixture，不依赖伪造“存在闭包”才能进入。逐条检查了以下oracle及Rust所有权/类型用法，未发现之前layout测试那类消费self后再使用的错误：

- small source会导入、canonical定义总数仍一；large source body编辑后caller完整Module不变而owner Module确实变；输出unit均独立validate，不只是检查分支或key。
- large callee与caller移到同file后，完整body仍共址且不产生跨unit clone；所有source移到单file后，prepare返回None，cached入口输出与原planner完整units/fallback相等。
- helpers全false时，local identity同时被两个source调用并由外部callback global取址，identity/state/callback仍只定义一次、实际共址；无关第三source保留第二unit，避免测试仅因全图退化一个unit就误通过。
- 将当前未被caller使用的source返回值改为不存在的SSA，prepare拒绝且输入逐字不变，覆盖新准入对坏model的最近边界。
- policy测试分别对default off/before-and-after、incremental=false三种merging与原planner做完整units/fallback比较；仅显式incremental+after产生新bounded布局且所有unit有效。

复核了原address/local-state/indirect-address/reverse-dependency/cycle/copy-budget测试：原需要None的反例仍因mandatory local并组合成一个成员组、坏model或预算返回None，并未因删除“无helper”准入而显然失效。新增测试的borrow、临时format转owned、index整型推断、owner查找均静态成立；实际编译/测试结果由root精确发布SHA的运行收据提供。

文档一致性提交只将原“多文件且有闭包”描述改为普通纯函数/无helper亦适用，build.md同时明确cost32、大body canonical调用和需独立运行质量门。现有门限、验证预算和已拒绝实验没有改写。

## 解释边界

新机制针对已证实的入口全图导入放大；大source不再导入，理论上能缩小caller对象的编辑失效，但这不是已经测得的收益。跨unit优化可见性变少会改变生成机器码，cold总CPU、完整CLI/resident请求和真实程序runtime均需重新核验。

每file仍按file_id%unitcount落owner，原N64 k4全落owner1、k16只落owner1/5的事实保留；不能把这些格子写成4/16个独立对象同时变化。single-file快退避免为原巨大单文件场景新增planner集合分配，但本审核没有做性能测量来保证其耗时为零。

最终结论限定于1679/tree1e599b的源码可继续验证；主线采用仍须精确head构建、测试、完整输入功能与独立运行质量/性能数据。
