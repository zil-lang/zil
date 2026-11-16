补充结果审阅：checker5000wide cold/broad/restart；native2500 positions/broad/recovery

2026-11-16；候选 4a23edc7d06bc18cbed1fb958fdaacd886d9607a。只读取既有收据、源码与 diag02；未运行编译、测试、计时或修改收据。范围仅为这六项损失定位，不增加既有 774/258/71 验收数量，不改变 main5105 ResourceLimit 导致的整体 accepted=false。

**结论：本组最大正式绝对差是 checker5000wide restart 的 +1357.588 ms。它仍执行完整 producer/RawInfer/PlainCT/Analyze，FunctionSession 实际只恢复命中 101/5005，不能把 hydrated 理解成整链免执行。native2500 的三项则分别是位置搬迁成本、广泛变更后的前后端重算、以及局部恢复后仍发生的 transform/partition 与对象编译工作。各项 Rust 总差不是某一个 ZIL 阶段的净损失。**

正式数字来自 `out/runs/restart-performance-stage-analysis-01.json`，并逐项读取两个 block 的原始 candidate response（共 12 份）。下表单位均为 ms；C/R 是两个正式 block 的中位数，差额列保留配对 block，不能据两个样本推断稳定分布。

| 正式请求 | C | Rust | C−Rust | block00 / block01 差额 |
|---|---:|---:|---:|---:|
| checker5000wide cold | 1621.930 | 807.345 | +814.585 | +579.877 / +1049.293 |
| checker5000wide broad | 1280.198 | 864.271 | +415.927 | +323.428 / +508.426 |
| checker5000wide warm-restart | 1385.004 | 27.416 | +1357.588 | +1312.090 / +1403.086 |
| native2500 positions | 727.074 | 530.948 | +196.126 | +412.830 / −20.579 |
| native2500 broad | 1599.150 | 1365.541 | +233.609 | +211.251 / +255.968 |
| native2500 recovery | 480.906 | 331.445 | +149.460 | +164.379 / +134.542 |

正式阶段中位数如下。Residual = report.seconds − phase 合计 − 已记录 driver 合计；本组正式 driver_timings_ns 为空，因此 residual 尚未细分，不能称作纯 I/O、纯序列化或某个确定算法。wall 与 report 的间隙另计，没有塞入 residual。

| 请求 | Expand | CT | Types | Analyze | Verify | Codegen | report residual |
|---|---:|---:|---:|---:|---:|---:|---:|
| check cold | 210.280 | 306.024 | 527.740 | 93.511 | 56.169 | 无 | 425.009 |
| check broad | 371.955 | 71.217 | 246.118 | 110.407 | 82.650 | 无 | 394.784 |
| check restart | 163.323 | 44.341 | 388.851 | 163.457 | 139.140 | 无 | 481.563 |
| native positions | 222.866 | 24.597 | 39.398 | 22.263 | 32.549 | 56.261 | 322.389 |
| native broad | 116.344 | 25.735 | 48.909 | 34.133 | 34.438 | 821.483 | 509.587 |
| native recovery | 21.211 | 11.671 | 3.780 | 0.498 | 14.619 | 293.430 | 132.893 |

Parse（各项不足 1 ms）以及 native Lower（2.363/2.914/0.034 ms）未列在表内；表不是逐行完整加总。RawInference 位于 project_pipeline 的 CT query 内，不能再把其工作量算作独立于 CT 的额外时间。Backend 子计时已经包含在 Codegen；LLVM ProfileStats 又包含在 object 子阶段中。

**checker5000wide：真实执行范围及可证明边界。**

Cold 两个 block 均为 disk=missing、context_rebuilds=1：producer 执行 102 modules / 5005 lower items；RawInfer full_runs=1、各类 body/name/dependency 检查均 5005；PlainCT shape/body checks 各 5005；Types body_checks=5005、FunctionSession executed=5005、reused=0；Analyze product_function_queries_executed=5005、reused=0；Verify=1。没有 native backend。`analyze.reused_functions=4997` 是另一层局部分析计数，不能覆盖 product 层的 5005 次执行。

这里能证明完整当前前端生产和冷建成本；不能证明 Types 的 527.740 ms 是稳定的首要单点：两个 block 的 Types 分别 174.786/880.694 ms，CT 为 474.244/137.803 ms，residual 为 388.149/461.868 ms。同样工作计数下阶段波动很大，不能用均值排列替代因果证据，也不能把全部 +814.585 ms 归给 Types。

Broad 两个 block 均走 resident sparse（context_rebuilds=0、sparse_batches=1），并非 FunctionStore 全量重建：producer 执行 100 modules / 5000 items，另复用 2 modules；RawInfer 检查 4800、复用 205；Types 检查 4800、位置重绑 205、receipt_tree_updates=5005；Analyze product 执行 4804、复用 201。PlainCT 仍 shape/body checks=5005，receipt_context_matches=0。可定位为广泛源码变更的 producer 与 body 工作，并有位置/receipt 全范围更新；不能称为“sparse 路径没有命中”，更不能从 4800 与 4804 的差值断言分析重复错误。Expand 两块 493.659/250.250 ms，Types 163.945/328.292 ms，residual 391.954/397.613 ms；未分账部分仍显著。

Restart 两个 block 均 disk=hydrated、attempts=1、Verify=1，但 producer 102 modules / 5005 items、RawInfer full 5005、PlainCT full 5005、Analyze product full 5005 均真实执行。Types context_rebuilds=1，actual restored_reused_functions=101、executed_functions=4904；body_key_executions=101、body_key_visited_nodes=44758，inventory/context declarations 为 5005。这里不是 5005 次 full-body hashing：当前 `zil-types/src/incremental.rs:1569 worthwhile` 对 tiny leaf 不建立函数缓存 key，只有非 foreign 且 ≥4 statements 或含分支/循环的函数进入该 key 路径；`keys_recording:1592` 仍遍历 inventory 并收集实际 observed reads。冷/重启都只有 101 个 body keys，与该 shape gate 相符；未逐函数证明 4904 个排除原因，也未证明扩大缓存覆盖会更快。

实际入口是 `zil-driver/src/project.rs:1523` Types query → `TypesPushSession::apply_with_function_cache` → `apply_inner` 无 resident sparse context 时 `rebuild` → `check_prepared_program_recording`；真实结果再形成当前 recording/checkpoint。`project.rs:1635` 之后只有真实 Types/Analyze epoch 和 retained producer 的 RawInfer/PlainSource checkpoint 都成立才标 Hydrated。该标记证明完成当前恢复条件，不抵销生产费用。

重启 Types 两块 117.802/659.901 ms，Analyze 63.298/263.615 ms，Verify 223.890/54.389 ms，residual 713.317/249.808 ms。固定工作量下波动也存在；最大的确定损失位置是“需重建的前端 + 尚未分账 driver 工作”，不是可精确分配的某个唯一阶段。diag02 是 native2500/5105 重放，不是 checker5000wide，不能借其时间填补这里的 residual。

**native2500 positions：复用成功仍有全范围位置处理。**

两块完全一致的工作计数：producer reused_lower_modules=102、executed_lower_items=0，但 rebound_core_function_deep_clones=2505、rebound_restored_item_deep_clones=2505、cloned_source_map_entries=41071；RawInfer 0 checked / 2505 reused；Types body_checks=0、position_rebindings=2505；Analyze product 4 executed / 2501 reused。PlainCT shape/body checks=2505。后端 lowered_functions=0 / reused=2505，却 representation/scoped-thread checks=2505、source_projection_traversals=2505。故“0 body/0 lower”不等于整条编译路径零工作。

源码位置：`zil-expand/src/incremental.rs:1098` 的 placement.rebind 分支实际搬迁并统计 Core/syntax/source map；`zil-types/src/push.rs:370` 对旧 TypedFunction clone/rebase；`zil-ct/src/plain_session.rs` 的 receipt shape/body gate 未复用时走 walker 检查；`zil-backend-llvm/src/emit/incremental/representation.rs::check_representations` 和 source projection 仍需匹配当前输入。

后端 transforms=true、partition=true、reused_link=true。原始 final_artifact 为 reused_program=true、before_codegen=true、current_preflight_reused_units=16；object_reports 为空、object_cache hits=0/misses=0。准确描述是完成产物在对象编译之前复用，不能写成“16 object hits”。`build.rs:1030` 的 previous.matches → final_product::authorize → restore/publication 是实际出口。machine=44.833 ms 并不意味着 transform 重跑；该父区间包含当前 machine admission/lowering/reuse 路径。partition 仅 0.013 ms。

主要正式位置为 Expand 222.866 ms 与 residual 322.389 ms；Codegen 只有 56.261 ms。block00/01 residual 为 451.152/193.625 ms，C wall 为 950.805/503.342 ms，而 Rust 为 537.975/523.921 ms，第二块已快于 Rust。没有证据把这一项定义成稳定 +196 ms 的后端损失。

**native2500 broad：大量当前前端工作，后端主要并非全部 LLVM 重编。**

两块 producer 执行 101 modules / 2501 items、复用 1 module；RawInfer full 2505；PlainCT 检查 2501 / 复用 4。Types sparse body_checks=2302、position_rebindings=199、retained=4、signature_updates=1、signature_readers_checked=1；Analyze product 和 backend lower 都为 2302 executed / 203 reused。representation 检查 2501 / 复用 4；source projection 遍历 2505。

transforms=false、partition=false；两块均重建 16 partition units，但 object cache 是 15 hits / 1 miss。正式 Codegen 821.483 ms 内，machine=375.950、partition=143.592、object=183.660、diagnostic=64.215、link_execution=41.491 ms。故“16 partitions rebuilt”等价于“16 LLVM 编译”的解释错误。唯一 miss 对象仍有真实成本：object_reports 的 optimize/emit 中位合计约 130.464 ms，已在 object 内，不能再加到 Codegen。

`zil-backend-llvm/src/build.rs:440–499` 的当前 identity + TransformPolicy gate 未复用，实际执行 fold_checked_add_chains / fold_known_leaf_calls / validate_module / machine_call_classes::canonicalize；`build.rs:531–556` 的 PartitionInput（含 transform policy）也未命中 completed output。可证明完整 transform 与 partition 工作发生；machine 是复合区间，没有足够子计时把 375.950 ms 全归给 canonicalize。call_classes 当前图为 eligible=2500/classes=126/rewritten=2374，数值是图属性，不是计时或工作次数。

残差两块 743.779/275.394 ms；Codegen 989.690/653.276 ms；Rust wall 1755.314/975.768 ms 同样大幅变动。两块差额都为正，但不能用候选某个阶段直接解释 Rust 配对差的全部 233.609 ms。

**native2500 recovery：保留成功 epoch，有局部前端恢复，也有真实下游扩散。**

该请求是长流程 error 后的 recovery，不是进程重启，也不能按名称假定等于某个仍在四槽中的完整历史产物。两块 attempts=1，producer 只执行 1 module / 25 items、复用 101 modules；RawInfer 检查 2 / 复用 2503；Types sparse body_checks=2、position_rebindings=23、retained=2480、context_rebuilds=0；Analyze product 1 executed / 2504 reused，backend lower 也是 1 / 2504。它不是旧失败路径 clear 全 session 后的前端全量恢复。

不过 transforms=false、partition=false、reused_link=false；16 partition units 重建，items 复用 2569；object cache 两块都为 1 hit / 15 misses。representation 检查 25 / 复用 2480，source projection 遍历 227 / 复用 2278，显示局部输入经当前下游依赖继续传播。正式 Codegen 293.430 ms：machine=91.441、partition=52.550、object=123.086、link_execution=18.747 ms；Types+Analyze 仅 4.277 ms。定位应放在下游 transform/partition/objects 以及 residual 132.893 ms，而不是函数类型检查失去复用。

对象 ProfileStats 的 optimize+emit 两块分别 112.743/108.233 ms（中位 110.488 ms），是 object 内的实际 LLVM 成本。当前 call graph 变为 classes=127/rewritten=2373；`machine_call_classes.rs::canonicalize` 会按实际调用关系和 source-order representative 重写。计数与代码支持“全图转换可能扩大局部变更的产物影响”，但本次未做每个 object key/partition 的逐字段 provenance diff，不能断言这 15 个 misses 全由 call-class 代表改变造成，或断言它们都本可避免。

三项 native 的 native_history_retained_epochs=0 仅描述 SDK opaque history，不描述新增四槽 MachineProduct 的占用/淘汰。长流程跨多个 private/interface/broad/error/recovery，不据请求名预设四槽命中；复用与否以 reused_machine_module/transforms/partition 和 final_artifact 实际字段为准。

**diag02 只补阶段位置，不替换正式比率。**

已读 native2500 三项独立 profile response 与 Rust stderr。候选 driver 中 semantic_candidate_publication / source_ingress 分别为 positions 164.674/93.156 ms、broad 130.317/19.580 ms、recovery 65.619/3.851 ms。实际 semantic serialized_functions 分别 2505/2501/25，serialized_function_plans 为 0/2/0；各有 1 次 manifest write 和 1 个物理 pack write。`project.rs:1663–1689` 成功后 store.save 仍执行，而 sparse 分支不重新发布 FunctionStore 全 manifest。说明 resident/sparse 成功仍有 semantic publication 和 ingress 工作，足以指认正式 residual 的候选来源；不能把这些独立诊断数直接移填到正式 residual，或声称已解释完它。

这三项 Rust 诊断 stderr 有当前编译阶段；positions codegen_to_LLVM_IR 约 0、type_check 109 ms/MIR borrow checking 92 ms；broad LLVM_passes 167 ms；recovery LLVM_passes 10 ms。这些只是独立重放的阶段证据，Rust query/LLVM/链接计时有嵌套或异步区间，不能机械累加，也不能与上述正式 C−Rust 作减法。沿用已确认限制：diag02 Verify 扰动；Cargo Fresh 可重放旧 time-pass stderr，旧输出不算本次执行；此处不借 warm-restart 的旧 time-pass 作归因。没有新增 Rust 或候选测量。

证据索引（均相对 `/workspace/zil-push-main`）：正式摘要 `out/runs/restart-performance-stage-analysis-01.{json,md}`；check 原始 `out/runs/restart-performance-check-5000-wide-01/requests/block-{00,01}/{000-cold,012-broad,016-warm-restart}/candidate/response.json`（请求编号以 rows.jsonl 的 directory 为最终依据）；native 原始 `out/runs/restart-performance-native-01/workflow/2500/block-{00,01}/requests/{011-positions,013-broad,015-recovery}/candidate/response.json`；诊断索引 `out/runs/restart-performance-diagnostic-02/diagnostic-rows.jsonl` 及其 response/stderr 路径。正式配对采用原 rows 的非 sustained 行，未把独立诊断、运行时速度或额外持续循环混入。
