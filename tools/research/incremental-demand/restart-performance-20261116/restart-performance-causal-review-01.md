# 本轮性能结果判读清单

2026-11-16，Astra。范围：current `4a23edc7d06bc18cbed1fb958fdaacd886d9607a` 路径及现存 pinned Rust/Cargo 源码；供 root 的 main9170/current/Rust 串行同窗 nativefull、retain 和独立 check 结果审核使用。仅阅读源码，未执行测试、计时或性能脚本；本文不判定任何速度胜负。

1. **先固定比较单位，再定位阶段。** 分别比较冷编译、进程重启、首改、resident 编辑和历史恢复；同时保留 restart + first edit 的联合成本。确认相同模块规模、优化/CGU/线程/入口、缓存是否保留及 native 产品选项。保留失败 attempt 和独立正确性结果；旧窗口只作背景，不替代本轮配对。进程外 wall time 包括启动/报告输出等；JSON `seconds` 是 compiler 请求内时间，不能当成完整命令 wall time。Rust 若是 Cargo Fresh，需明确标注该单元没有调用 rustc，而不是推断 rustc 内部所有查询命中。

2. **FunctionSession 磁盘命中仍有当前全量工作。** 路径是 `project.rs::project_pipeline → TypesPushSession::apply_with_function_cache → rebuild → FunctionSession::check_prepared_program_recording`。`try_sparse` 成功才绕过此 rebuild。

   - rebuild 当前执行 shape/name/signature/context 检查。`Reuse::prepare` 在 recording 模式走 `keys_recording`，不走 producer memo 快路；`key_context` 遍历当前 namespace/import/声明，`function_key_inner/BodyKey` 遍历合格函数的完整 Core body、相对位置及实际 callee signature，顺便记录 reads。即使 CFG 全命中，仍可能有大量 `body_key_executions/body_key_visited_nodes`。tiny/foreign/不合格函数不生成可复用 key，继续普通 Builder。
   - `Reuse::get_inner` 完成当前绑定、必要 span rebase、六列原费用重放后才计 hit；`restored_reused_functions` 是实际使用 decoded Entry 的次数，仅用于 fallback 分类。它不是解码数量，也不意味着本次刚读磁盘。`Recording::record/finish` 仍建立当前 owner/readers/费用树、检查 post obligations；`adopt_output_anchor` 做实际全字段比较后才保留旧 Typed owners。recording/anchor 没有独立细分 timer，不能把 Types 的剩余时间全叫作 hashing。
   - `FunctionStore::restore` 只在 session 空时恢复；items decode 校验/解码并计算 scalar CFG result fingerprint，保留已有 wire encoding。`Entry::canonical()` 是惰性的 source-relative rebase，和 Core body-key hashing 是两种工作。`publish_products` 遍历当前最终函数，缓存支持的 scalar result identity 可复用；其他函数仍可能走 `raw_result_fingerprint`。
   - `encode_item_products` 遍历 manifest entries；已有 matching encoding 可直接共享，否则 `canonical()` 后 `encode_raw_function`。`FunctionProduct::canonical_function` 可供 semantic publication 复用同一 canonical owner。故“CFG hit”不等于“无 encode/manifest/I/O”，“encoding reused”也不等于“无解码/遍历”。push 仅成功 rebuild 发布 FunctionStore；成功 sparse 不重发旧 FunctionSession manifest，但仍可能发布本次 semantic candidate。

3. **Cold 与 resident producer 的证据不同。** push 磁盘 whole candidate 走 `LoadedProject::Hydrating`：真实 discovery/Parse/Expand/RawInfer/PlainCT/Types/Analyze 按当前输入执行或消费自身真实缓存，decoded Typed 仅为输出 anchor。`disk_cache="hydrated"` 表示完成后全等并具备当前上游/Types/Analyze checkpoints，不表示所有 provider 未执行。磁盘 FunctionStore 不携带 Expand/CT/Analyze 会话。

   resident 经 `reuse_project_frontend` 检查完整 DB trace、源字节/options，`retained_producer_epoch/activate_producer_epoch/activate_types_epoch` 恢复真实会话。若 Candidate raw 槽被逐出但真实 AnalysisEpoch 仍在，`product_checkpoint_for` 提前恢复，普通 Analyze query 仍可标 Executed，而 `analyze_product` 返回 `product_complete`，函数重算为零。旧 epoch 被 `prepare_project/trim_history` 淘汰、输入/limits/context 不匹配则仍需当前生产；仅有磁盘同值不算 resident。

4. **按以下字段区分命中与补建；null 表示未报告/未执行，不能一律解释成零成本。**

| 问题 | 当前 JSON 证据及解读 |
|---|---|
| whole query 是否执行 | `disk_cache` + `query_events`；`cache_hit` 只是汇总布尔，不能定位省掉哪层。`source_ingress.resident_frontend_hits`、`frontend_dependency_reads` 辅助确认 resident 查证。 |
| Types CFG 是否真正命中 | `frontend_stats.function_types.{reused_functions,restored_reused_functions,executed_functions,cfg_same_span_reuses,cfg_relocated_functions}`；再看 `body_key_executions/body_key_visited_nodes` 和 `context_*_hashed`，发现“命中但全量 key 遍历”。 |
| push 是补建还是稀疏 | `types_push_work.{context_rebuilds,sparse_batches,body_checks,retained_functions,signature_readers_checked,position_rebindings}`。`context_rebuilds=1` 配高 CFG reuse 仍是当前 push context 补建；whole Types query hit 可令该对象为 null。 |
| 上游/Analyze 重算范围 | `frontend_stats.producer` 的 executed/reused lower items、modules，结合 `raw_inference_work`、`ct_plain_work`；Analyze 看 `frontend_stats.initialization.{shared_program_reuses,product_function_queries_executed,product_function_queries_reused,product_global_obligations_executed}`。`ct_plain_work` 只计 plain admission，fallback 的普通 CT 另在 phase 时间内。 |
| 编码与持久化 | `function_cache_encoding.{serialized_functions,reused_functions}`，`function_cache_publication` 与 `semantic_cache_publication` 的 encode/guard/I/O、written/existing、physical_pack_writes、manifest_writes；语义缓存的 `canonical_functions/bound_functions` 是传输选择，不是 Types 执行次数。 |
| backend 是否在对象命中前重做 | `backend_work.{reused_machine_module,whole_machine_input_functions_compared,lowered_functions,reused_functions,reused_machine_transforms,reused_partition,reused_partition_recipe,reused_object_keys,rendered_diagnostic_items}`，并结合 representation/source-projection 计数。只有 object hit 不能推出这些层命中。 |
| native 与最终产物 | `native_codegen.object_cache/object_reports`、`native_product_selection`、`backend_work.native_history_*`、`native_codegen.final_artifact` 与 `reused_link` 分开读。`native_codegen.call_classes` 是已完成图的  eligible/classes/rewrite 属性，transform reuse 会保留它，不能当作本次全量重写计数。 |

5. **四槽下游历史的成功与失败条件。** `emit/incremental/whole.rs::reuse_machine` 要求实际 Verified/LIR 输入、module metadata、functions/layouts、完整 limits、原初始账本、WholeOptions 及 closed scalar 域匹配；全值比较通过并按原 spent 扣费后恢复真实 LowerEpoch。非同一函数持久树可能仍产生 `whole_machine_input_functions_compared`，即使命中也非零成本。

   `MachineProduct.completed` 属于既有四槽 LRU，没有独立历史。`completed_machine_products` 先核对实际 `same_identity`；`build.rs` 再检查 TransformPolicy（backend/O级/incremental/closed/merging/entry/protected），partition 保留完整 PartitionInput/policy/environment gates，object keys 继续检查目录/late candidates。两条成功出口才附完成链，driver 暂存 session 在成功发布后提交。新进程无此内存历史、第五个历史逐出旧项、失败链未提交、角色/策略/输入不符，都会失去相应复用。`native_history_retained_epochs` 是另一个 native-product 历史的统计，不能当成这四槽 lower-history 的直接计数。

6. **计时只能按层展开，不叠加父子。**

   - `phase_timings_ns` 包含阶段 query/provider、digest/发布等，`driver_timings_ns` 是 phase 外的 ingress、checkpoint、cache read/publication 等。`seconds` 包含这两者和未归类部分。先读 `exclusive_costs.{segments_ns,unassigned_ns,overlap_ns,overlap_errors_ns}`；若 overlap 非零，保留异常，不将误差解释成算法收益。
   - `frontend_stats.function_types.total_ns` 是 Types phase 内部子区间；shape/context/key/CFG 细分也是子区间。尤其 `body_key_ns` 已包含在 `evaluator_ns`，不要相加；`inventory_ns` 已扣除 body-key 部分。`exclusive_costs` 用细分替换父 Types，并留下 `types_other`；Expand 的 producer timers 和 `materialization_ns` 同样包含于 Expand。
   - `function_cache_publication.encode_ns/guard_ns/item_io_ns/manifest_io_ns` 属于 driver 的 `function_cache_publication`；semantic 对应字段属于 `semantic_candidate_publication`。decode/其 result hashing 在 `function_cache_read` 内，不能再算成 Types body-key 时间。
   - `native_codegen.timings.total_ns` 位于 `phase_timings_ns.codegen` 内。backend 的 machine/partition/diagnostic/object/link-key/link-execution/other 是该层设计的互斥 wall buckets；partition 已扣 diagnostic。对象 worker 细项可能并行，不能将其 CPU/worker 时长逐项加回 wall time。object bucket 还包含缓存准备、调度，不等同纯 LLVM。
   - 顶层 phase/driver/function 工作已汇总 attempts，不能再加 `attempts[]`。backend/encoding/publication 等部分字段只描述最后 attempt；失败耗时仍应保留在请求 wall/phase/attempt 总账。当前 B2 fallback 仅实际 restored hits 触发，独立 fresh session 的第二次真实工作也要算进去。

7. **Pinned 对照只解释机制，不代替本轮数据。** 已阅读保留源码 manifest：Rust `58cf8f9c35b355199727627d07ae34a4e2846d67`、Cargo `5f94df4789f005f9a352888e8355ffc645b7ed0e`，位于 `/workspace/zil-recovery/rust-pain-audit-fresh-20261116-01/out/runs/rust-pain-audit-20261116-01/implementation/`。

   Rust `compiler/rustc_query_impl/src/execution.rs:408,459`：先 `try_mark_green`，再加载该 query 的值；green 但无磁盘值仍执行 provider。非 green 用 `with_task` 执行，完成值进入 `cache.complete`（同文件92）。`rustc_codegen_ssa/src/base.rs:1238`：旧 work product 存在且 CGU dep-node green 才选择 Pre/PostLto 复用。不能把“Rust incremental”统称为零前端或零 LLVM 工作。

   Cargo `src/compiler/fingerprint/mod.rs:459`：fingerprint/依赖及文件系统新鲜度通过时 `Job::new_fresh()`，不启动该单元 rustc。与 ZIL 当前独立 Verify/真实生产恢复不是相同工作边界；应照实比较用户可见总时间，同时标明差异，不能把 Cargo Fresh 当成更强语义证明，或据源码给 Rust 编造本轮阶段耗时。

收到结果后按“同窗总差额 → phase/driver 最大差额 → 对应执行计数 → 子计时”定位：例如 Types 命中但 hashing 高、Expand 真实补建、machine transform miss、partition miss、publication 成本或 link-key 工作。只报告字段能支持的归因；没有 Rust 阶段证据时，可指出 ZIL 的主要耗时阶段，但不声称该阶段解释了两端全部差额。本轮不施工，不要求新增矩阵。


正式窗口后的诊断边界补记：若正式 JSON 的 phase/backend timings 仍留下无法解释的 driver residual，root 可在**全部正式窗口结束后**，仅对 candidate 用 `--profile-compilation` 跑原 18 请求的单区块诊断。必要时另用 pinned Rust 的 `-Ztime-passes`（`RUSTC_BOOTSTRAP=1`，相同 O2/CGU16/jobs1，独立全新 target）定位阶段。这些诊断只解释阶段损失，不并入正式六窗比率；全新 target 的 cold 诊断也不能替代正式 warm/retain 对照。父子 timer 不累加，残差先核对覆盖范围及 overlap，不能直接等同可消除工作。当前 native 正式窗仍进行，不启动额外 compiler/profile；本审阅未启动任何诊断。

历史命中边界：`backend_work.native_history_retained_epochs` 统计 SDK opaque native-product history，并非新增 `MachineProduct.completed` 所属的四槽 machine history。cold → 8 次 private → positions/interface/broad → restore 的长序列，若产生超过四个不同保留 epoch，旧 cold 可被逐出；不得预设 restore 必然命中。以该请求的 `reused_machine_module/reused_machine_transforms/reused_partition` 及实际输入/policy gates 为准，不能用 native history 数量或仍在磁盘的 object hit 替代证明。root 当前通报 retain 72/24/10、`candidate_rust_accepted=true`，但 main 5.1k 预算仍失败；此为进行中状态记录，不构成全部正式验收通过或整体速度结论。
