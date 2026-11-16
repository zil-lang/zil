# 正式六窗与独立阶段诊断：结果审核

2026-11-16，Astra。只读审核 current `4a23edc7d06bc18cbed1fb958fdaacd886d9607a` 的正式结果、原始 response、LLVM object ProfileStats，以及 diag02 Rust stdout/stderr；未启动编译、测试、计时或修改源码。下面所有差额均为 candidate − Rust，正数表示编译请求等待更长。

**最大单请求绝对损失是正式 native 长流程的 5105 warm-restart：3562.134 − 42.525 = +3519.609 ms。** 这是实际重启后的上游/Analyze/machine/partition 重建与请求内其他工作，并非 LLVM 对象重编译：两区块均为 hydrated、Types 5101 个恢复 CFG 命中/4 个执行，但 Analyze 与 machine lowering 各处理 5105 个函数，transform/partition 未复用，对象缓存均 16/16 hit。完整 native 流程总时间较低，不能掩盖此局部损失。

**另一项稳定损失是正式 native 的 5105 八次 private 编辑：9.831954 s 对 8.688412 s，合计 +1.143542 s。** Types/Analyze 已真实稀疏命中；主要时间在 backend 全局处理及 LLVM 对象优化/发射。不能把这段损失解释为首改前端 miss。

1. **范围和数值核对。**

   正式来源为 `out/runs/restart-performance-{native,retain,clear}-01`，以及三个 `restart-performance-check-{2500-chain,2500-diamond,5000-wide}-01`。已读取 root 的 `restart-performance-stage-analysis-01.{json,md}`，并从各窗原始 `rows.jsonl` 独立复算配对；排除 sustained/runtime-profile 附加记录后为 774 原始编译请求、258 个三臂质量单元。258 个 candidate/Rust 配对的源身份一致，逐区块时间差与 stage analysis 一致，未发现数值不符；stage analysis 六窗 warnings 均为空。

   读取了 258 份 candidate 原始 response（check 窗部分在原始 row 内联），核对阶段总账；对 native/retain 的 5105 重启另外核对原 archived row 的 SHA256 及其 resident response 与独立 response.json 相等。下文阶段归因取原报告字段，不用 runtime 效率反推编译速度。

   native/retain/clear 的 runtime case 数为 51/10/10，71 个 Rust runtime gate 全部通过；三个 check 窗合计 306 行均 valid。三个 native 窗仍各自 `accepted=false`：main 的 5105 在 Expand 报 `E-RESOURCE-LIMIT: generic expansion budget exceeded`，共 36+12+12 个原始观测。其编译失败时间不算成功编译优势，也不能把 `candidate_rust_accepted=true` 写成全局验收通过。

2. **正式编译等待时间；两个完整区块先各自求和，再取中位数。**

| 窗口/范围 | Candidate s | Rust s | C−R s |
|---|---:|---:|---:|
| native 1000 完整18请求流程 | 7.165744 | 8.046677 | −0.880934 |
| native 2500 完整18请求流程 | 10.082064 | 10.016992 | +0.065071 |
| native 5105 完整18请求流程 | 29.476549 | 31.581859 | −2.105310 |
| native 5105 八次 private | 9.831954 | 8.688412 | +1.143542 |
| retain 2500 restart + first edit | 1.056898 | 0.447165 | +0.609733 |
| retain 5105 restart + first edit | 4.441371 | 1.008351 | +3.433020 |

完整流程包括约定的 error/recovery 请求；这是请求等待总账，不是“18 次成功编译”的均值。正式 compiler 请求 wall 不包含另外记账的 worker 启动、cache cleanup 和 runtime gates。

| 主要局部损失 | Candidate ms | Rust ms | C−R ms |
|---|---:|---:|---:|
| native 5105 warm-restart | 3562.134 | 42.525 | +3519.609 |
| retain 5105 warm-restart | 3196.505 | 13.264 | +3183.240 |
| check preset5000 wide warm-restart | 1385.004 | 27.416 | +1357.588 |
| native 2500 warm-restart | 892.821 | 57.529 | +835.292 |
| check preset5000 wide cold | 1621.930 | 807.345 | +814.585 |
| retain 5105 restart 后 first edit | 1244.866 | 995.086 | +249.780 |

check preset5000 wide 的完整17请求等待为 C 7.573905/R 7.050026 s，差 +0.523879 s；2500 chain/diamond 分别为 C 4.174603/R 4.755087 s、C 3.033722/R 5.261316 s。不能把 native 总流程较低推广为所有编译工作负载都较低；check preset5000 也不能与 native 5105 的不同程序形状混池。

3. **重启的正式分账：真实补建，没有假报 query 命中。**

| 5105 重启的候选耗时，中位数 ms | native 长流程 | retain 短流程 |
|---|---:|---:|
| Expand | 891.704 | 295.303 |
| Types | 208.609 | 270.103 |
| Analyze | 110.505 | 152.205 |
| Verify | 74.886 | 105.006 |
| CT | 79.999 | 68.296 |
| Codegen 父项 | 1295.051 | 1546.990 |
| └ machine 子项 | 708.367 | 961.582 |
| └ partition 子项 | 299.068 | 388.194 |
| └ diagnostic 子项 | 185.835 | 89.551 |
| └ object 子项 | 24.651 | 18.680 |
| 请求内未细分残差 | 890.977 | 747.326 |

codegen 与其子项不能相加。表中未列全部小项；完整 signed residual 仍在 stage analysis 中。

`LoadedProject::Hydrating/project_pipeline` 继续真实上游阶段；`FunctionSession::check_prepared_program_recording → keys_recording → Reuse::get_inner → Recording::finish` 消除了合格 CFG 的 Builder 重建，但没有消除当前 Core key/签名读取/费用/owner 记录。重启的 5101 restored hits 与 Types context_rebuilds=1 同时成立。`AnalysisSession::analyze_product` 没有进程内旧 complete epoch，因此 5105 个 product function queries 执行；backend 同样没有进程内 machine history，进入当前 lowering、`fold_checked_add_chains/fold_known_leaf_calls/canonicalize` 及原 partition 路径。原始计数是这条执行路径的证据；现有 timer 不能再把 machine bucket 精确拆成每个 transform 的毫秒数。

两窗重启都是16个对象命中，object bucket 仅约19–25 ms。主要差额发生在对象命中之前和请求内其他工作，不能归因为 LLVM miss。native 的请求内残差为两区块 672.44/1109.51 ms；retain 为660.75/833.91 ms。正式未启用 driver profiling，不应将这些残差直接命名为某一个 API，或假定全部可消除。

4. **private 编辑的正式证据把损失定位到 backend。**

   native 5105 的两个区块均逐次只检查1个 Types body、1个 Analyze function、lower1个函数；其余5104个函数在相应消费者复用。八次合计 Types 2.614 ms、Analyze 1.718 ms，二者合计4.332 ms。Codegen 合计8.272640 s，占候选9.831954 s的约84.1%；其中 machine 2.385678 s、partition 1.683987 s、object 3.892648 s，余项含诊断、链接与其他工作。

   两区块每次 private 的 object misses 均为 `[16,15,16,16,16,15,15,15]`，每区块124/128个单元 miss；transform 与 partition 未复用。这说明局部前端/单函数 lowering 截断成功，但没有截断整个全局 transform/partition/对象链。具体代码位置是 `build.rs` 的 TransformProduct identity/policy miss 分支、partition gate 后的全局规划，以及对象 miss 后的 LLVM API 路径。未从这些字段进一步断言某一次 call-class 代表变化是全部16个 miss 的唯一原因。

   **原正式 object ProfileStats** 每区块求和再取中位：`optimize_ns` 2.347703 s，`emit_ns` 1.384705 s，`builder_ns` 0.041461 s，`preflight_ns` 0.035533 s；各对象 `total_ns` 合计3.832367 s，是 object父项3.892648 s的子集。O2优化/机器码发射明显大于 IR Builder 开销。此处 jobs=1；这些子项/total仍不得再加回object或codegen。

   retain 的 restart 后 first edit 同样是 Types/Analyze/lower 各执行1个；Codegen为1057.858 ms，其中 object546.177 ms，且16个对象 miss。因此“首改真命中”已成立，但“首改整个 backend 足够便宜”尚不成立。

5. **diag02 只解释细分机制，不替换正式比率。**

   diag02 为36 candidate +36 Rust，所有行 valid；manifest 明确 `formal_comparison=false`、没有运行 native input/runtime gates，正式 receipts 未改。diag01 的 setup 异常不作为一次编译计时或新样本混入。diag02 的18请求顺序是 native 长流程，private-001 是 cold 后首改，并非 retain 短流程的 restart 后首改。

   5105 diag02 warm-restart：候选 wall3026.566 ms；请求内 driver 细项包括 FunctionStore read142.940 ms、semantic read95.260 ms、source ingress84.850 ms、semantic publication135.721 ms、function publication23.717 ms，剩余未分类55.442 ms。它证明正式 residual 涵盖真实读取/解码/发布等工作；这些数值不能直接填进正式 native 的890.977 ms残差。

   同次 Types157.900 ms：5101 body keys、191558个 key visited nodes、5101 restored CFG hits、4个 Builder执行；body-key19.335 ms已经包含于evaluator19.335 ms，CFG reuse/rebase28.850 ms。FunctionSession total123.546 ms是Types的子项；剩余还包括记录完成、anchor及query等工作，不能统称 hashing。Function cache encoding为0 serialized/5101 reused；semantic publication却重新序列化5105个函数及5105个function plans，encode79.996 ms，尽管实际新pack写入为0。必须区分 CFG命中、编码复用和物理写入。

   2500 diag重启仅101个恢复CFG命中、2404个Builder执行，符合 `worthwhile` 的tiny-function排除域，不能套用5105的命中比例。diag的5105 Verify达724.169 ms，而正式 native重启仅74.886 ms；这类波动更说明不能把单区块profile值当作正式差额的精确分解或净优化收益。

   **Rust stderr 有已确认的缓存重放陷阱。** 两个规模各有 unchanged、unchanged-after-edits、warm-restart，共6条 `compiler-artifact.fresh=true` 的请求仍含旧 `time:` 行。5105 warm-restart wall17.202 ms、无 `Running rustc`，stderr的52条阶段时间却与上一条restore逐行相同，含 `total=5.982s`；这些不是本次Rust执行成本，必须排除阶段归因。2500重启同理。

   对确实运行rustc的5105 diag private-001，原stderr为type_check136 ms、MIR borrow59 ms、codegen_crate130 ms、LLVM_passes43 ms、finish_ongoing_codegen392 ms、link40 ms、total1480 ms。这些父子/异步阶段不可简单累加；其请求wall1507.271 ms也不能替换正式native首改Rust1206.714 ms。已核对实际命令 O2、CGU16、Cargo jobs1、`RUSTC_BOOTSTRAP=1/-Ztime-passes`。此证据与 pinned Rust58cf 的query green/load-or-provider、CGU work-product gate，以及 Cargo5f94的Fresh路径一致，不代表所有Rust请求都走Fresh。

6. **区块波动、总账和历史边界。**

   - 最大native重启损失在两区块分别为+3428.733/+3610.484 ms，方向稳定；retain重启为+3045.948/+3320.532 ms。retain首改差额却为+437.980/+61.580 ms，幅度波动明显；联合restart+edit仍为+3483.928/+3382.112 ms。
   - native5105八次private总损失为+1221.682/+1065.402 ms，两个完整区块一致落后。native2500完整流程为+78.222/+51.921 ms，约0.65%的中位差，宜称近持平；candidate自身两区块为10.908/9.256 s，不把这种小差额推广为稳健算法排序。native5105完整流程优势为−1091.778/−3118.842 ms，同样应保留范围。
   - 258份candidate报告未发现phase+driver超过请求内seconds；142份backend timings的子项之和与total完全一致；diag02全部36份candidate的exclusive overlap及overlap_errors为空/零。该核对证明所报时间账目可按现有层级解释，不证明每个timer覆盖了所有潜在子操作。顶层已汇总attempts，不能再把attempts加一遍；原失败和runtime时间不抹去。
   - `native_product_selection.enabled=false`，理由为 `below-minimum-average-lir-work`；SDK opaque history 的三个计数为0不说明四槽MachineProduct未实现。长流程restore的真实证据是machine/transform/partition均未复用，Types/Analyze/lower处理约5000个变化函数，最终对象16/16命中。cold之后超过四个不同epoch，不能预设旧cold仍在槽内；源码有界恢复机制不等于本长序列必须命中。单独的 unchanged 可复用其他层，也不能替代restore请求自己的计数。

本次审核结论：正式数字与配对可复核；局部主要落后点是重启真实恢复，以及5105连续private编辑的全局backend与LLVM对象工作。没有证据支持把这些差额归结为Types首改失效，也没有证据支持全局accepted或全工作负载领先Rust。阶段数据给出成本位置，不等于各阶段全部可被安全删除的时间；本轮不提出实现或新增测量。

证据入口：[正式stage JSON](/workspace/zil-push-main/out/runs/restart-performance-stage-analysis-01.json)、[正式stage Markdown](/workspace/zil-push-main/out/runs/restart-performance-stage-analysis-01.md)、[diag02摘要](/workspace/zil-push-main/out/runs/restart-performance-diagnostic-02/diagnostic-summary.json)、[diag02原始行](/workspace/zil-push-main/out/runs/restart-performance-diagnostic-02/diagnostic-rows.jsonl)。典型原件：[native5105重启response](/workspace/zil-push-main/out/runs/restart-performance-native-01/workflow/5105/block-00/requests/017-warm-restart/candidate/response.json)、[diag5105重启response](/workspace/zil-push-main/out/runs/restart-performance-diagnostic-02/diagnostic/5105/block-00/requests/017-warm-restart/candidate/response.json)、[Rust重启stdout](/workspace/zil-push-main/out/runs/restart-performance-diagnostic-02/diagnostic/5105/block-00/requests/017-warm-restart/rust/stdout.bin)、[Rust重启stderr](/workspace/zil-push-main/out/runs/restart-performance-diagnostic-02/diagnostic/5105/block-00/requests/017-warm-restart/rust/stderr.bin)。
