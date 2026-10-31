# CP32 独立正确性审计

结论：**Pass，范围为精确冻结提交的静态设计与正确性控制**。绑定 `3ba131efdeffe85ba5fccd1c177745d812a6ce61`，父实现为 CP29 `a0f38d2bca8f01fc19ff1f2fa0a42c3983c223b8`。审核者为 cp25_evidence，不是实现作者；没有修改作者 worktree，开始与结束时均为干净状态。生产代码只改 `program.rs` 和 `program_memo.rs`，另三文件为说明及控制。

本结论不包含性能采用或 native 执行结论。CP29 的默认配置回退已经有完整独立复算；CP32 必须另作完整控制和计量，不能继承 CP29 的缓存命中数作为提速证据。

## 设计边界

CP32 将成功 epoch 的保留策略改为稳定模块顺序下的部分保留。完整环境仍必需：所有声明签名、声明节点、传递模式、foreign 合同、模块顺序、imports、root、target、全部 Limits 和调用方 context 都逐字段绑定。即使一个模块因容量未保留，它的声明仍在环境内，变更仍会使整个 epoch 的复用资格失效。

旧模块的白名单与结构权重只在完整环境相同、整个 CoreModule 相等时复用。公开 API 调用者伪造或重复 source_fingerprint 并不足以命中。完整环境相等时沿用其权重也成立：计费的字符串、模块/声明/参数/导入数组及长度、foreign noescape 列表均由匹配身份决定；正文和局部名字另算在模块 source_weight。

容量饱和与语言资格分开。容量数值饱和后仍继续检查当前模块正文及 Typed 输出的尾部；unsupported 形态不能因为前部太大而被跳过。语义诊断仍由原 Builder 产生。全局环境本身超容量时直接放弃该优化，仍走原编译路径。

检查过程中仅保存 FunctionId 与预算使用收据，先完成选择，再复制入选新模块的 TypedFunction。最终输出已按 FunctionId 排序；快照在 lending::populate 前形成，保存时还原 Builder 原始函数名，避免把项目诊断标签当成下次 Builder 输出。取不到完整对应函数会放弃 memo，当前有效 Typed 输出仍保留。

完整 receipt 预留容量、每模块向量和全局环境都纳入结构计费，包括最终被省略的模块。部分选择可跳过过大的条目并继续考虑后面较小的条目。旧条目使用不可变 Arc；本次输出复用了合法旧条目但下个 epoch 不再保留它，仍不影响语义。旧 epoch、新 epoch、真实 IR 与 query 缓存可能同时存在，所以 **64 MiB 是单 epoch 的结构权重界限，不是 RSS 或全部瞬态内存上限**。

原 CP29 的共享预算全量检查、原子扣减、额度不足时返回旧 Builder 的首错机制未改。新 epoch 的交付仍受后续 lending、analysis、取消及完整请求成功约束；失败不能替换旧成功 epoch。

## 冻结后的实际控制

在根授权的正确性窗口内，使用已有 target-cp32，以 jobs=2、测试单线程运行。两次 Cargo 命令都直接绑定冻结 checkout，全部日志与命令已保存。

| 控制来源 | 通过 | 失败/忽略 | 说明 |
|---|---:|---:|---|
| CP32 私有小容量单元控制 | 3 | 0/0 | 部分选择、尾部资格、共享预算首错 |
| 原 CP29 driver 控制在 CP32 冻结上复跑 | 12 | 0/0 | 全身份、FFI、伪 hash、配额、取消/分析失败、显式清空等 |
| CP32 新 driver 控制 | 2 | 0/0 | 真实 64 MiB 部分保留、编辑后分析与 Verify 仍运行 |
| 原非作者 CP29 consumer 重新链接 CP32 | 3 | 0/0 | 12 个混合 epoch、四配额维度、语义错误后恢复 |
| 新非作者 CP32 consumer | 2 | 0/0 | 选择集合缩小/扩大、未保留末模块仍参与语义与身份 |

共独立执行 17 项作者控制和 5 项外部 consumer；其中复用旧控制明确列出，不计作新语言能力。Cargo 的另外 18 项单元测试被 filter 排除，未声称已运行。

新增非作者控制使用公开 check_program/check_program_with_memo API，并逐次将完整 TypedProgram 与 fresh 结果相等比较：

- 根模块增大后，生产容量下保留模块数真实经历 **812 → 805 → 812**。扩张时仍可合法使用旧 epoch 的 7,299 个函数，但只为新根复制 1 个快照；恢复时重新获得被省略的条目，实际复制 64 个函数，复用 7,236 个。各步完整 Typed 均与 fresh 相同，仍存活的原 epoch 统计未被改变。
- 未保留的末模块改变可见性，使全局复用函数和 source receipt 复用都归零。随后在同一末模块引入未知名字，warm/fresh 都在 **FileId(1024)、span 424..435** 返回同一 `E-UNKNOWN-NAME` 首错。旧成功 epoch 不变，恢复请求成功且无新快照复制。

这一 1,025 模块正确性样本与正式 tree1024x8 性能语料不同，不能混用命中数或运行时间。审核没有追加 native 控制，也没有把 correctness test 的耗时记入性能。

## 复核文件

机器结论及逐源文件摘要在 `review/CP32-INDEPENDENT-REVIEW.json`。作者控制命令/退出码在 `review/CP32-INDEPENDENT-AUTHOR-CONTROLS.json`，完整输出在同前缀 TYPES/DRIVER 日志。非作者控制源码为 `review/cp32_independent_consumer.rs`，它导入既有 `cp29_independent_consumer.rs`；所有 rustc 参数、rlib 摘要、二进制摘要、执行退出码在 `review/CP32-INDEPENDENT-CONSUMER-EXECUTION.json`，原始输出在相应 COMPILE/TEST 日志。

静态审读及上述控制未发现阻断问题。性能、完整语言覆盖和产品总门禁保持原有独立结论，等待后续候选窗口。
