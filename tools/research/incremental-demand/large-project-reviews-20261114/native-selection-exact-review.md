# Hegel driver native product 成本策略独立复核

结论：**ACCEPT（限定静态独审范围，无阻断发现）**。当前成本策略没有把 capture 选择变成语言、LIR 或 machine 的授权；重→轻→重成功以及失败/取消的 history 提交边界在代码上成立。该结论不代表整个 e005777aa、SDK 或性能目标已验收。

本复核者不是该 driver 策略作者。按本次指令只读源码、提交对象和已有证据；未执行构建、测试、bench 或 Git 命令，未修改任何源树。唯一写集为本报告。

## 精确候选与范围

- 候选：`e005777aa63d8a3031a9024eaa12af5376d6904c`。
- 提交 tree：`0136f049e870e19d71dc0a6f987fc38d12700344`。
- 源树：`/workspace/zil-push-main`。
- 直接解析并校验本地提交/tree/blob 对象的 SHA-1，确认以下工作文件字节与该提交相同；不是仅依据当前分支名或工作树状态。

| 被审文件 | 提交 blob | 文件 SHA-256 |
| --- | --- | --- |
| `crates/zil-driver/src/session.rs` | `12ebb3e9e79f111f1d7a3ee6ea4c40e5fd8b6f18` | `a679080b3d47a8d43aefed5f0a19fd7c60b0d1685ca36857bf141850f027ed4d` |
| `crates/zil-driver/src/lib.rs` | `0493be6809ceed1dc73bc04a688eb79be838dca8` | `6232c17c9cec6e9849a5ca9f2ee74173643b67c74089f45a133ebfd084e71373` |

主审为上述两文件的 native selection、报告及发布调用链；只读追踪 backend `build.rs`、`incremental.rs`、`final_product.rs`、`emit.rs`、driver `artifacts.rs`、Verify 入口与 LLVM API 入口以核实消费者合同。e005 的 scalar source admission 等其他改动不在本报告接受范围。读取了仓库 AGENTS 和恢复的 goal/workflow 文件；不将恢复看板当作当前门的成功收据。

## 成本选择与显式 API

`session.rs:18–98` 的选择要求同时满足 `push_reuse`、编译 feature `native-products`、LLVM API backend、O2。否则直接拒绝自动 capture。满足前置条件后只遍历当前完整 LIR 的 functions/blocks，并读取 `instructions.len()`；没有按指令重走 AST、body hash、旧 receipt 或历史 owner 猜测。

有 body 的定义计入分母，空 blocks 的声明不计入；判定为当前总指令数 ≥ `64 × body_functions_counted`。计数加法和阈值乘法均检查溢出，溢出时拒绝 capture。没有浮点舍入、默认 limit 修改或检查费折扣。新增选择成本为 O(functions + blocks)，不按指令逐条遍历；它仍是实际新增工作，不能从报告中的指令数推断逐指令扫描成本。

`session.rs:1365–1367` 只把选择结果写入本次新建 `BuildOptions.native_products`。完整 O2、function merging、entry、codegen、object cache、target 与 native-link 配置继续沿原调用链传递。全模块平均值是可选 retention 的启发式：不能保证每个 codegen unit 都获益，也不能保证热点大函数不会被大量小函数稀释；这是待实测的性能边界，不是正确性授权。

显式 backend `BuildOptions.native_products` 及 `zil_llvm_api::compile_module_with_native_products` / `compile_module_with_native_cohorts` 没有调用 driver 阈值函数。显式请求仍进入原 native 路径，feature 不具备时仍按原 API 返回 unavailable，不会被该策略静默降成普通请求。driver 的 `push_reuse` 是可选复用策略入口，没有新增“必须 capture”的公开参数被阈值覆盖。

## 成功、错误、取消与 history

| 请求序列/结果 | 实际状态边界 | 静态结论 |
| --- | --- | --- |
| 重成功 → 轻成功 → 重 | 轻请求配置变为 `native_products=false`；普通完成路径调用 `NativeProductHistory::commit(false, …)`，清空 epochs 和 recency。后一个重请求不能恢复已清掉的 native allocation。 | 正确。对象缓存可以独立命中；此时允许没有新 native product，不能把 `enabled=true` 解读成已生成 product。 |
| 重成功 → 轻失败/取消 → 重 | backend 是上一成功 session 的 clone；失败/取消不回写 clone，因而上一成功 native history 仍在。 | 正确。请求失败本身不应退役上一成功 history。 |
| 轻成功 → 重失败/取消 | 同样只丢弃本次 backend clone；不能留下半成品 native history。 | 正确。 |
| 复用关闭后的成功 native build | 发布成功后将 `self.backend` 清为默认；失败则保留旧值。 | 正确。 |
| Verify/Lower 等提前返回或失败 | 没到 native 发布与 backend 回写。 | 不冒充成功 native build，也不执行新的 native-history 提交。 |

关键依据：

1. `session.rs:1396–1406` 在本次局部 backend clone 上构建；`1420–1446` 的 Link cancellation、dump、artifact validation 和 `publication.commit` 都在 `1457–1467` 回写 `self.backend` 之前。正常错误返回不能先清掉或替换上一成功 native history。
2. `incremental.rs:552` 起的 history 用不可变 Arc epoch；需要改共享 epoch 时 `Arc::make_mut`，recency 属于各 clone。`commit_with_activity` 的 disabled 分支实际同时清空 epochs/recency；不是只把统计归零。
3. `build.rs:1292–1308` 完成 native history commit 仍只影响局部 backend；driver 发布失败会丢弃整个局部值。`artifacts.rs:556` 起保留 cancellation 检查、两文件 publication 与失败 rollback。合作式取消以这些检查点为界，不承诺打断已经运行的外部 linker。
4. 特别核对 `build.rs:839–908` 的 final-product 提前返回：`final_product.rs:55–88` 的 Configuration 保留完整 BuildOptions，仅排除输出目的地；其中包含 `native_products`。重→轻的 true→false 因此不能命中旧重配置并跳过清理。轻→轻命中时，前一次成功轻请求已清空 history；重→重命中则保留原成功 history，符合可选复用语义。

## 当前独立检查与统计边界

`session.rs:1220–1240` 先调用当前 `typed.verify`，成功取得 VerifiedModule 后才 Lower、选择 capture 和调用 backend。`TypedInput::verify` 传递当前 candidate、target、limits；选择和报告均不能替代它。Verify 内原有同强 owner receipt 或逐义务检查仍受 Verify 自己的身份、target、limits 绑定控制。本策略没有新增跨请求 Verified 授权，也不要求每次重新扫描已被独立核精确绑定的全部字节。

backend `emit.rs:210–243` 先保留原预算 reserve 和 `bind_lir`，再允许 machine 产品复用；`bind_lir:1078` 起实际比较 schema/module/verified fingerprint、target、functions、closures、resources、layouts。选择开启与否均不绕过此入口。

native capture 路径保留当前 structured preflight / Builder 与 LLVM before/after Verify；`native.rs:628–648, 666–667, 748` 分别可见当前 preflight、首轮 Verify、原 `default<O2>` 和后置 Verify。对象缓存 hit 则在 `build.rs:1554–1563` 绑定当前 machine preflight；final-product 快路径在 `844–850` 调用 checker-owned authorize。后二者保留原有“复用已验证产品、当前重新绑定”的合同，不伪称本请求重新执行了 LLVM；原 receipt 也分别说明该范围。

`lib.rs:276–303` 明示 selection 是 capture-cost 信息；全库引用检索只见写入、序列化、初始化/重试清理和测试读取，没有由 report 反向授予 Verify/preflight 的消费者。`restart_attempt:528–548` 清除旧 selection，避免旧 attempt 数据冒充当前最终 attempt；`None` 表示未到选择点，`enabled` 表示选择请求，不表示成功、已生成/复用了多少 native 产品。选择后的错误报告可以仍有 selection，这是有意的执行信息，不是成功授权。

## 已有测试覆盖与未测边界

本次只审测试内容，没有运行，不能记为通过：

- `session.rs:1660` 的 `average_lir_work_boundary_and_overflow_only_select_optional_capture` 覆盖 2-body 下 127/128 临界、零 body、乘法溢出及加法饱和。
- `session.rs:1701` 的 `heavy_cheap_heavy_current_native_requests_keep_checks_and_retire_products` 使用真实 source、可执行文件与 fresh 对照，核重/轻/重输出、每次 Verify、轻成功 history epochs/payload 归零、第三次重无旧 history restore。每步后插入真实语言错误，并比较 backend 未被替换。
- `incremental.rs:888` 的 `native_history_is_bounded_lru_and_checkpoint_owners_remain_immutable` 覆盖 history clone 隔离、disabled 清空及旧 checkpoint 保持不变。
- 原 `retained_native_publication.rs:265–289` 覆盖 Link 后取消与 retained publication 恢复；`llvm_api_driver_integration.rs:271` 覆盖真实 API codegen/link 后取消不得发布；driver 原测试还有双文件保留及 late artifact failure。它们没有直接断言本次重/轻切换中的 native history 内容。

**非阻断覆盖缺口**：新增用例中的错误发生在选择之前；没有直接组合验证“重成功 → 轻请求选择后在 Link observer 取消或 publication 失败 → 重恢复”并比较完整 backend/history，也未直接组合“轻成功 → 重请求选择后取消”。代码上的 clone/发布顺序已经给出隔离依据，但不得把上述现有测试称作这些组合已实测。成本阈值的实际冷/热净收益及不同 unit 分布仍由 root 的精确候选测量裁决。

已读现有 `out/runs/large-source-native-chain-14-e005777aa63d/{binding,completion}.json` 与 `/workspace/zil-recovery/chain14-gates.log`：source 绑定本候选，`all_gates_valid=false`，记录停在 Expand 121 pass / 1 fail。本报告未据此声称 native-selection 测试或整链门已通过，也不将该 Expand 首败归因于本策略。

本轮交付仅此独审报告；源树继续停写，无生产补丁、无构建结果、无性能通过声明。
