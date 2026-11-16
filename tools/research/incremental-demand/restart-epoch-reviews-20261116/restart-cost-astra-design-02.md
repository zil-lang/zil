# Restart cost：现有 FunctionCache 接入与重复全链消除设计

冻结提交最终复核：`5a324acdd0bb0b3a9c31f6ad3b73ff025c7fe4eb`。已用 `git show` 阅读该提交全部七文件 diff，包括新增测试源码；未执行测试。按 root 最新指令停止后续设计，当前结论是 **2 处需要收口的失败边界，不能给出无 blocker 结论**：

- **B1：life 缓存试探失败重复计费。** `zil-types/src/incremental.rs:998` 在当前 State 上执行 `lifetimes::compile`；同文件 `1000–1035` 多个 miss 分支不回滚，随后 `program.rs` 的普通 Builder 又执行 life 编译。尤其原始累计预算不足以 replay 时，已消耗的 life `work_left` 再次扣费，会改变原失败费用/诊断边界。该提交没有排除这种声明，最终 `closed_signature` 拒绝 push Context 发生得太晚。此为原 FunctionSession 缺口被新 push rebuild 引入，不是已运行的反例。最小改动：recording 请求下，在 `get_inner` 任意计费前令带 `life` 的函数 miss；或者完整实现试探账本事务。不影响当前无 life 标量缓存命中。
- **B2：push rebuild 的失败统计与已有 rejected-candidate 回退未接完整。** `zil-driver/src/project.rs:1532–1536` 仅在 `Ok(update)` 内赋 `function_work`；FunctionSession 已在失败返回前保存本次 stats，但 driver 未取它，Types 出错时 report 丢失已经执行/复用的工作。外层 `project.rs:465` 的重试条件要求该计数，因而不成立；另一个既有缺口是 Types 失败时 `report.phase` 仍为 `Ct`（1471，成功后才赋 Types），查询 event 不改变它，重试的 Types/Analyze phase 条件也会挡住。最小改动：保存调用结果，在失败 rebuild 返回前收集本次 FunctionSession stats；成功 sparse 不读旧 stats。按实际失败阶段选择已有 immutable-input fallback，保持取消/IO/limit 不重试的原约束，并保留失败 attempt 的实际工作记录。无需扩大缓存格式或增加授权。

其余本次关注项未发现新 blocker：完整当前 key 遍历成功后才保存 observed reads；miss 用真实 Builder observations；六列 replay 在当前累计限额内原子检查；Recording 绑定本次真实 owners；Types 查询失败/取消回滚 function/push checkpoints；成功结果才写 FunctionStore，sparse 不重发旧 manifest；恢复只在空 FunctionSession 进行且使用独立 push v3 目录；非 push 原路径保持。新增测试源码覆盖普通缓存恢复/readers、后续编辑、签名错误、全局 block 限额、失败后恢复及 driver 首次编辑，但未覆盖上述 life speculative-miss 与 driver Types 失败计数/回退边界。统一正确性批次由 root 执行，本审阅不重复运行。

以下是冻结前已经完成的设计记录，保留溯源，不是要求 root 等待的后续工作。

2026-11-16，Astra 独立、只读源码审阅。基线为 bcf409b37；本次同时阅读共享工作树中 root/Peirce 正在集成的 FunctionStore v3、TypesEpoch.functions、FunctionSession + Recording 改动。它们尚非本报告独立验证过的冻结提交。未运行构建、测试、格式化、计时或性能脚本；未修改仓库。本文只写外部设计记录。

结论：当前接法能用现有真实单函数磁盘 CFG 缓存，在一次当前 Types 执行中生成 push 状态，不必先完整重建 Types 再另跑一遍填缓存。恢复边界和 observed reads 的接法合理；费用方面有一处原有 speculative miss 的 life 计费风险，应作局部收口。它减少的是冷启动 Types 的函数体检查，不能直接消除 Expand/CT/Analyze 或冷 machine/partition。下一条明确的重复全链优化，是补齐已有真实 machine history 与单槽 transform/partition 之间的断点；不应等待这个后续工作才收本轮 FunctionCache。

1. **本轮 FunctionCache 的实际调用链与恢复边界。**

   `crates/zil-driver/src/semantic_cache.rs::FunctionStore::new` 已允许 push，并使用独立的 `function-types-v3-push`。`restore` 仍只在 FunctionSession 为空时读取现有 bounded manifest/items/packs；有活跃的真实内存缓存时不再重复解码磁盘基线。沿用现有 raw-cache 信任边界，没有新增可从磁盘构造的 ProducerEpoch、VerifiedModule 或 SourceMachine 权限。

   `project.rs::project_pipeline` 现在调用 `TypesPushSession::apply_with_function_cache`。`try_sparse` 成功直接返回；只有 rebuild 才调用 `FunctionSession::check_prepared_program_recording`，后者把同一个 `Reuse` 和 `Recording` 传入原有 `program::check_resolved_program_recording`。这是正确的单次组合位置：shape/name/signature/header/post 检查在当前请求执行，body 命中走已有 raw CFG 复用，miss 走原 Builder。

   `incremental.rs::key_context` 仍绑定 target、完整 limits、root、模块/函数 ID、imports/alias/name inventory；函数 key 绑定当前 body、相对 span、自身及读取的当前声明。非 identity inference、shape 展开为 owned、非 scalar/unsupported key 等不能据此建立完整 push 图；实际 checker 继续执行，`Recording::finish` 不完整即不发布 Context。磁盘条目只省略符合既有边界的 body 构造，不作为 terminal Types 查询或上游 epoch 的替代物。

   `push.rs::rebuild` 在真正完成检查后才做全字段 `adopt_output_anchor`，再由 `Recording::finish` 绑定本次 ProducedProgram 的 function owner/body revision 和最终 Typed owners。不能把 decoded Typed、缓存 key 或旧 owner 直接写成当前生产证据。当前实现保持了这一顺序。

2. **Observed reads：新接法填补了原先不能直接组合的缺口。**

   `incremental.rs::keys_recording` 复用 `function_key`/`BodyKey::call` 的当前 Core 遍历。调用经当前 module/import alias/signature tables 解析，再记录当前 dense FunctionId；只有完整 key 构造成功才插入该函数的 read set。失败的部分遍历不会留下可授权 cache hit 的完整 read set。foreign/unsupported 调用仍拒绝 key。

   `program.rs` 的 cache-hit 分支从 `Reuse::signature_reads` 填入 `observed`；缺失集合则把 `recording.observed_complete` 置 false。真实 miss 仍由 `Builder.observed_signatures` 记录。集合来自当前源输入，不是从优化后的 Typed CFG 猜测，更不是把旧 read set 换一个 epoch 标签。Core 保守地多记录合法分支中的调用，只会扩大失效范围。

   因而将入口 `observed_complete` 初始化为 true 是有对应条件约束的：命中必须有完整当前 key reads，miss 必须由实际 Builder 记录，最终还要通过 closed-state/cardinality/owner 等全部 `finish` 条件。不能为了扩大命中率跳过这些限制，也不需要增加 persisted readset 格式。

3. **费用：正常命中组合正确；life 试探失败需要收口。**

   `Reuse::get_inner` 使用条目保存的六列实际 Builder 费用，扣除本次已真实执行的 life header 费用后，经 `closures::State::replay_accounting` 在当前累计余额中原子检查并重放。不是每个函数重新获得 limits。`program.rs` 的 `push_before` 在 `get` 前，`Recording::record` 在本次 body 及后续相关检查后取差额，因此 push row 包含真实执行/重放的当前区间。`finish` 仍检查整个 context/body ledger 的组合约束。

   **具体风险：** `get_inner` 调用 `lifetimes::compile(function, state, limits)` 后，bound 不符、费用不能相减、span rebase 失败或 replay 超预算等分支会返回 `Ok(None)`，但此前 life 编译可能已经消耗 `work_left`/`nodes_left`。随后 `Builder::new` 又调用 `lifetimes::compile`。`key_context` 没有统一排除 `f.life`；primitive 参数且 life parameter tags/result 均为空的声明也可以到这条路径。损坏候选或累计预算导致 miss 时，缓存尝试可能改变普通编译应有的费用/错误边界。这是旧 FunctionSession 的边界问题，被新 push 路径继承；不是已测得的 5105 标量负载故障。

   最小落地修正：在请求了 `signature_reads` 的 recording 模式中，`get_inner` 在任何计费前，对 `function.life.is_some()` 直接返回 miss，使用一次真实 Builder。这样不改 nonpush 行为，也不影响无 life 的普通标量命中。若要支持该域，再让试探使用临时账本，只有完整命中才提交；不能仅在外层错误时恢复 FunctionSession，因为同一函数内部的 miss 已污染当前 checker 账本。原始 `replay_accounting` 自身的失败是原子的，问题是它之前的 life 费用。

   稀疏成功路径保留旧 FunctionSession 是合理的：它表示最后一次真实 rebuild 的 body cache，不表示它的统计/manifest 已更新为当前 Typed 全量。当前 driver 已只在 `context_rebuilds != 0` 时 `bind_products`/`store.save`；不会为了刷新磁盘基线再走一遍完整 Types。TypesEpoch.functions 可保存这个真实缓存并由当前 keys 再验证。不要把 push `Row.cfg` 当作原 FunctionStore Entry.cost 导出，两者记录区间不同。

   Types 查询失败/取消时 driver 恢复 function/push checkpoints；FunctionSession 只在成功检查后替换 entries；不完整 Recording 不产生 push epoch。Hydrated 仍需当前 Typed/Analyze 和真实上游 checkpoints，不能因 FunctionCache 命中就提前设置。

4. **冷重启与 warm 历史恢复应采用不同的现有路径。**

   冷重启采用上面的单次 FunctionCache + Recording。现有磁盘 FunctionItem 只有 raw Typed CFG、key、原始费用及编码边界，不含原始 Parse/Expand/RawInfer/PlainCT/Analyze 会话。它能节省合格函数的 Types Builder 工作，不能凭空省去这些其他阶段；也不能先终止于 semantic candidate，然后把旧基线的全链补建藏到第一次编辑或后台。

   warm 恢复优先用 `session.rs::reuse_project_frontend`：当前完整 query dependency trace、源字节/options 检查、`retained_producer_epoch`、`activate_producer_epoch`、`activate_types_epoch` 都已存在。root 正处理重复快照/whole candidate 恢复，本报告不另开实现。

   需要保住的短板有两个。第一，`prepare_project`/`trim_history` 可在大项目超过 raw capacity 时删除历史完整阶段根；已删除的 A epoch 无法靠调整 A 恢复时的调用先后复活。若保留 A→B→A，应在既有有界容量内成组保留真实 Project/ProjectCore/ProjectTyped/Candidate 根及可达 metadata，而不是从 decoded A 伪造 epoch，也不是无限增长新历史表。第二，`activate_types_epoch` 目前恢复 functions/push/publication，却不在 Analyze provider 前恢复匹配的 AnalysisSession；`finish_typed` 在 Analyze query hit 后才恢复它。若真实 Typed epoch 与它保存的实际 Candidate 关联仍在、但查询 raw 槽缺失，可提前恢复那一份真实 session，让 `analyze_product` 的 `product_complete` 路径返回完成产物，再由普通查询发布当前依赖。关联必须经 matching epoch、实际 program/candidate 和 `product_checkpoint_for` 核对；没有真实 checkpoint 就正常执行。

5. **已定位的 backend 重复全链：历史 lower 恢复到了 A，下游单槽还停在 B。**

   `crates/zil-backend-llvm/src/emit/incremental/whole.rs::reuse_machine` 已有 bounded `machine_history`，核对当前 Verified/LIR 输入、完整 limits、初始账本和实际 spent 后，恢复 A 的 machine 及 `LowerEpoch`（context identity、products、entries、output bodies 等）。

   但 `incremental.rs::BackendSession` 的 `transformed: Option<Arc<TransformProduct>>` 与 `partition: Option<Arc<PartitionProduct>>` 各只有一个槽。`build.rs` 只查当前单槽：transform 必须 `same_identity`，否则执行完整 `fold_checked_add_chains → fold_known_leaf_calls → validate_module → canonicalize`；partition 不匹配又进入 stable prepare/prune/rename/partition。A→B→A 可以恢复 A 的 lower，却因单槽保存 B 而重跑 A 已经完成过的整个下游链。这是具体的缓存层级断点，不是 LLVM 对象 miss，也不是微分配问题。

   **下一条最小完整 chain：** 在 BackendSession 内让真实完成的 TransformProduct、PartitionProduct 随已有 machine 历史有界保留。lower 恢复后按实际 `ScalarIdentity` 和 transform 策略查对应完成产物，再按原 `PartitionInput` 条件选择 partition，回接现有 diagnostic/object-key/link 路径。所有 transform 相关 options、source eligibility/protected entry/task 集合必须继续绑定；保留完整现有 equality/environment 条件，不仅比较一个 call-classes 开关或源 hash。历史淘汰要与 machine history 协同计入保留成本，不能借新的 Arc 根永久保活已淘汰的全量 machine。首次无完成产物仍走普通链，失败/取消不发布半成品。

   这条改动复用内存中的实际完成结果，不增加磁盘 SourceMachine 授权格式。它能避免 warm A→B→A 的重做；不能改善一个全新进程第一次构造 machine 的同一部分成本。

6. **普通单函数 edit 的后续完整 transform 链，不应只缓存最后的 rewrite。**

   当前 `same_identity` 要求所有函数 machine/source identities 相同。任一相关函数变化即失去整个 TransformProduct，即便 lowering 已仅处理一个函数。`MachineOutput::changed_items`、持久化函数产品和实际输出 equality 已提供变化输入；缺的是各 transform 的依赖状态。

   可在现有 TransformProduct 中保留真实各阶段产物：checked-add 的函数级输入/输出；scalar fold 的实际 callee 依赖和反向调用者；call-class 的当前 address/protected inventory、依赖 DAG、完整 normalized Function key、class members/representative；最终 rewritten 输出。变动沿这条真实链传播，输出全等处截断，再进入原 PartitionInput/recipe 条件。先做函数级局部 pass，再处理依赖闭包，不能用一个函数的最终机器 body 直接假定所有全局变换不变。

   `machine_call_classes.rs::canonicalize` 明确按 DAG height、source order 选代表，full Function Eq 决定 class，cycles/其调用者保持 unclassified。局部 body 改变可能使代表退出、旧类换代表，并影响未改源码的所有调用者；address uses、globals、protected exports、函数增删/重排也可能改变 eligibility。只有保留并更新这些实际依赖才能省掉全量分类/重写；遇到未覆盖的环境变化就回原完整 pass。这是后续范围，不是本轮 FunctionCache 的前置条件。

   不建议直接删除重复出现的 `validate_module`。checked-add、scalar、call-class、stable-identity 检查之间可能已修改模型，不是同一次原始输入检查。只有确认模型未变时才能在一次调用的局部流程共享既有检查结果；不能用前一阶段的验证替代后一阶段对不同模型的验证，更不能把它扩成新的持久化证明体系。

7. **现有结果的解释及工作计数。**

   本轮沿用 root 提供的现存 5105 restart 数值，没有重新读取大文件或计时：object 16/16 hit；machine 1.087s、partition .315s、diagnostic .080s、link-key .041s；Expand .295s、Types .148s、CT .069s、Analyze .091s。它们指向对象缓存命中之前仍发生的前端和 machine/partition 工作；不能归因于 LLVM 对象重编译，也不能承诺 FunctionCache 单独消除全部冷重启开销。

   `build.rs::BuildResult` 明确把 `call_classes` 标成 completed transformed graph，并在 transform reuse 时保留该统计。因此 eligibility 5100/classes 156/rewrite 4944 不是本次工作量计数；需结合已有 `reused_machine_transforms` 才能判定是否实际重跑。不因这个区分追加测试或计时。

   任何优化的 restart + edit 总账仍包括 discovery/读取/解码、当前检查、费用重放、持久化和 backend 链。把首次真实补建挪到 edit、idle 或后台不算消除成本。此前 bcf 已证明首改真命中，但冷重启成本回退仍真实存在；旧 f19 与新结果窗口不配对且 Rust control 漂移，不能据此宣称确定的净算法收益或全场景领先 Rust。

本次交付是源码边界审阅与可实施的改动位置。当前优先收 FunctionCache 单次组合及 life miss 费用边界；warm 真实 epoch 复用由 root 推进；后续完整链优先补 lower-history 到 transform/partition 的断点。无新增测试/计时请求，无源码修改。

**冻结增量确认（2026-11-16）：** 已只读核对精确提交 `d7a978ed8b78aaad47737f0a167a4c772b07f31f`，父提交为 `5a324acdd0bb0b3a9c31f6ad3b73ff025c7fe4eb`，共八文件。确认包含计费前的 recording + life miss guard（B1 收口、nonpush 不变）、绑定真实 Typed/Candidate 双 owner 的 Analyze provider 前恢复、真实费用 budget fixture，以及既有四槽 MachineProduct 直接拥有的 completed chain。TransformPolicy/protected 集合同时进入 transform 与 PartitionInput gate；正常完成和 final-product 成功早返回两出口均调用 `remember_machine_products`。未发现本次增量新增的 Arc 环、无界历史保活、失败半链发布或 policy 漏绑 blocker。已阅读对应测试源码，未运行构建、测试、计时或性能矩阵。

此结论不关闭先前 B2：该冻结提交没有修改 `project.rs`，push rebuild 的 `function_work` 仍仅在成功 `.map` 中收集，原 Types 失败阶段/回退接线仍未在此 SHA 修正。故本次是“所列修正已入冻结提交、无新 blocker”，不是“此前所有 blocker 均已关闭”。不追加设计或验收任务；统一正确性批由 root 负责。

**B2 最终关闭（2026-11-16）：** 已只读核对精确提交 `4c82f4485052fa32af2e898e98a6406de4aba180` 的 `project.rs` 与 B2 测试源码。当前 Types query 前设置 `report.phase = Types`；保存 `apply_with_function_cache` 的 Result，并在失败或成功 rebuild 时收集本次 FunctionSession stats，成功 sparse 不读取旧统计。原 SafeReject/Unsupported + reused>0 回退条件、取消/IO/limit 排除及同一不可变输入重试保持。102 文件测试新增首轮真实 reuse、Types 阶段与 rejected-function 原因、两次 attempt、最终错误与 fresh 一致及后续恢复断言。**B2 源码审阅 blocker 已关闭**，取代上一段的未关闭状态；本次范围未发现新 blocker。未运行任何构建、检查、测试或计时。root 通报 Types110 全过、backend299 依据相对 b197 无源码差异复用、driver 尚在运行；这些是 root 的统一验收状态，不是本审阅独立执行的结果。此前失败记录保留，不追加矩阵。


**磁盘候选事务修复冻结确认（2026-11-16）：** 已只读核对精确提交 `4a23edc7d06bc18cbed1fb958fdaacd886d9607a` 全部六文件差异，本次事务范围未发现剩余 blocker。Entry 私有 disk-origin 标记在 legacy/items 两 decode 入口为 true、实际 Builder 保存为 false；仅在真实命中且原累计费用重放成功后增加 `restored_reused_functions`，JSON 和 attempt 汇总均包含该字段。重试仅由本次实际 restored hits 触发，Types/Analyze、SafeReject/Unsupported 及取消/IO/limit 排除条件保持。fallback 使用相同容量的独立 session 和同一不可变输入，成功才替换原会话；失败不再 clear 原会话，并恢复 function checkpoint，原有容量淘汰规则仍照常执行。新增/调整测试断言针对实际 restored hits 与汇总，未修改原五个恢复失败的断言。此为对先前 B2 接线结论的事务边界补充；首败记录保留。未运行任何构建、检查、测试或计时，不新增设计或性能矩阵；本次回归结果由 root 的统一批次确认。
