# Retained restart：前端会话缺失与安全补齐边界

只读独立复核。源码读取HEAD `afc97106d3b9ce9dc8eaa5a529c3dde72900e199`；与f19fc847c比较，本报告涉及的project.rs、semantic_cache.rs、session.rs、Expand incremental、CT plain_session、Types inference_receipts/push均无差异。未执行编译、native、测试或Git写入；仅写此报告。goal本地ref不存在，未获取/切换。root正在归档证据，本报告不启动性能窗口。

## 处置

**现有磁盘Whole-Typed命中不能直接恢复Expand/RawInference/PlainCT/TypesPush的逐函数成功epoch。拒绝用Typed合法性、当前Verify成功、源hash或重新分配Arc冒充这些生产凭据。** 这是一处已定位的跨重启增量状态缺口；未发现本路径因此产生错误结果，也未证明所有下一编辑差额都由前端耗时造成。

已有真正的驻留ProducerEpoch可安全激活；磁盘格式没有该epoch。可实现一个保守整链实验：连续编辑的冷会话首请求走真实前端建立epoch，同时保持对象缓存可用。它是重新生产/预热，不是免费磁盘恢复；必须计入首次请求及整个序列成本。

root提供的f19结果（本次未重读原始收据）：5105 retained restart 1.597s/Rust .0239s，下一私改2.773s/1.508s；clear restart3.798s/6.386s，下一私改1.196s/1.275s。ZIL两请求算术和分别4.370s和4.994s，retained仍少.624s；单看下一编辑慢1.577s便主张清缓存是错误的。Rust两序列也不是同一缓存状态。费用桶须使用本轮真实互斥计时，不把旧窗口4.777s当新窗口归因。

## 精确路径与可用/缺失数据

1. `zil-driver/src/project.rs:2225–2258`的单文件或完整discovery receipt命中返回LoadedProject::Semantic，绕过正常parser。已有完整当前ProjectSources（SourceUnit字节、FileId/ModuleId/path、root和paths）及当前options，不等于已生成ParsedSource。
2. `project.rs:712–770`导入Types查询，构造 `ProjectTypedOwner::restored` 后走finish_typed并成功返回，早于775行起Parse、Expand和CT。`session.rs:150–155`明确epoch=None。这样会发布当前Whole-Typed raw query，但不会自动建立它绕过的前端子阶段状态。
3. `semantic_cache.rs:83–104,239–253,409–480`存取TypedProgram＋CandidatePlan：typed module/函数、interfaces、root_functions、task_interfaces。它没有CoreProgram、parser lossless tape、Expand阶段receipts、原始推断访问费用或CT shape/body费用。packed分段改变IO布局，不补这些字段的语义。
4. 真正驻留路径 `project.rs:269–332`要求Expand raw owner、ProducedProgram与epoch.core关联、完整当前sources、target和limits，然后激活producer/materializer/raw_inference/plain_source及producer_syntax。`ProducerEpoch:95–130`包含实际ParsedSource列表、ExpandSession、materializer和消费者checkpoint；`1405–1435`只在真实CT生产/对应checkpoint成立后附加消费者。跨进程没有这些owner，不能让retained_producer_epoch接受decodedTyped。
5. Expand需要 `incremental.rs:371–382,546–606` 的Environment（source身份、headers、imports含span、limits、target、context、fused模式）、阶段nodes/work收据、声明slot/原始字节、FunctionOwner/BodyRevision与lower绑定。正常ParsedSource::parse (`project.rs:337–357`)才产生parser owner；公开AST或Typed不能倒推出它。
6. RawInference `inference_receipts.rs:39–135,182起`明确禁止decodedCore或下游Typed铸造收据；需要ProducedProgram的ExpansionEpoch/InputContext、逐函数owner/node/span/location及closure nodes/work、names、dependencies费用。PlainCT `plain_session.rs:17–94,106–180`需要真实Core及hint headers、shape_cost、body nodes/work、实际hint lookup键（包括负查询），并在当前SourceProgramContext与递减allowance中消费。结构性Typed Verify无法证明这些上游访问费用。
7. TypesPush `push.rs:63–72,92–125,535–550,660起`需要PreparedProgram/producer关系、signature/readers、shape/CFG费用、原始输入绑定和其实际返回Typed owner。checkpoint_for明确拒绝另行解码的相等Typed。直接“把每个Typed函数塞进去”既缺原始输入，也缺费用/读者边。
8. 旧FunctionStore不是现成桥：`semantic_cache.rs:747–750`在push_reuse时直接None；`project.rs:1490起`push分支调用TypesPush.apply而非FunctionSession。旧恢复格式含Typed函数与六项Types费用，但不能补Parse/Expand/CT或TypesPush的完整上下文。仅取消这个None并不会打通当前链，也不应为此次问题新增hash授权。

## Rust实际对照：加载哪个查询，就发布哪个查询

固定官方Rust `58cf8f9c35b355199727627d07ae34a4e2846d67`：

- `compiler/rustc_query_impl/src/execution.rs:418–435`先try_mark_green，随后取得该query的值。
- `459–519`中try_load_from_disk_fn成功则采用loaded value；缺少可加载值则执行该query provider，并校验预期结果hash。green并不保证值已经在内存。
- `try_execute_query:254–270`无论值来自磁盘还是provider，都调用job_guard.complete；`ActiveJobGuard::complete:86–92`通过cache.complete(key,value,dep_node_index)发布到当前查询缓存。

因此Rust的已加载值可供当前会话后续消费，且查询依赖/独立磁盘值使实际编辑能按需重建。**它没有从一个下游query值凭空反推所有上游session、费用或parser身份。** ZIL本次Whole-Typed raw发布本身已经与这部分相似；短板是没有上游可安全恢复的细粒度数据与对应入口。Rust的hash一致性检查也不是ZIL独立Verify的替代。Cargo Fresh不启动rustc时更不能用上述内存发布解释其约24ms。

## 一个可实现且不增加授权体系的修复实验

选择“仅需后续编辑的冷push会话，一次正常前端初始化；保留native对象缓存”。在显式研究/连续编辑策略下，当没有真实可激活ProducerEpoch/TypesEpoch时，在导入Whole-Typed之前选择正常源码路径：

- 路由要同时覆盖discovery的Semantic早返回与project_pipeline的Store早返回，不能只绕过其中一处；复用已读取的当前字节，仍执行原ordered discovery/parse与项目预算。
- 正常Parse→Expand→RawInference→PlainCT→TypesPush生成实际owners、dependencies和fees，沿既有capture/activation/publication路径发布；采用此实际生产的Typed进入原Analyze/独立Verify/native路径。不要先导入Whole-Typed query再指望同key执行fresh producer；那会再次被query命中截断。
- 保持对象缓存及语义缓存文件，既有对象查找仍需当前完整machine/预检链；不提前恢复磁盘machine权限。一次请求只编译一遍，不先完成cached native再递归重编当前项目。
- 错误/取消按正常阶段和原事务规则处理，不能为维持“cache hit成功”而吞掉真实前端失败。未知域保留正常路径；不从费用不足的缓存receipt补满预算。

这个实验确实能建立下一编辑所需的真实前端epoch，所需数据/生成入口都已存在。代价是重启时执行原前端；是否降低两请求总等待尚未量化，也可能负收益。只需显式原有研究策略下实验，不默认开启，不宣称实现Rust式跨进程细粒度恢复。

**拒绝另一种伪收益：**把整段重建移到未计时idle/background，或等下一编辑时先重建旧source再编当前source，却只报告后半段。延后旧epoch构造还会多做一次旧项目生产，当前证据不支持这条路线。

集中验收条件：保留对象缓存的restart→私改总等待与各阶段实际工作；真正冷/retain/clear分开；错误恢复、取消、接口/别名/遮蔽变化、声明移动、限额N/N−1、不同target/flags；fresh结果及独立Verify一致；峰值内存/留存磁盘不遗漏。当前报告不执行这些检查。

## 内容绑定

- project.rs SHA256 `a5098f0e230565c4ac619b911395d61cac9b502304c12af1b38e9b63da7a947b`
- semantic_cache.rs SHA256 `c7a7ea029b5fa78e05c537650d347075e476985c586f9d565cae1e58d158767a`
- session.rs SHA256 `629dfce2bd63b1c226eff9c5a0de6668a384848a56ac532272d997c080ee82a1`

READY。证明缺口与性能状态缺失已分开；没有宣称新的Source→Machine权限或未测收益。
