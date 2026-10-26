# schema8 集成回归：迁移五个旧 raw IR 反例夹具

## 基线、范围与原因

基线为 `aaeabc128e4120fcd70bb7403cee6d2deb939fda`。正式首轮验证为
31/32 门通过，`rust-tests` 中五个旧 raw IR 测试失败。原始失败日志保留于
`out/runs/validation/20261023T154846Z-43b4e5e9/rust-tests/stdout.txt`，本补充不改写该轮结果。

schema8 源码生成显式根/词法 scope 与 storage activation。旧夹具仍按先前布局插入指令，
导致结构检查先于其原本要测试的借用、移动、unsafe 或 Trap 检查拒绝；另有夹具仍假定
第零条指令为 UnsafeEnter。当前失败证据未显示生产语义缺陷。本补充仅迁移测试夹具，
不修改生产代码、规范、schema、预算、诊断优先规则或接受集合。

写集限定为 `crates/zil-driver/tests/` 的 `borrowed_closures.rs`、`impl029.rs`、
`impl042.rs`、`impl044.rs`、`review042.rs`，本说明，以及
`evidence/impl/056/fixture-migration/**`。不删除用例，不放宽原精确诊断断言。

## 修改前固定的迁移约束

| 夹具 | 结构迁移 | 保留的验证目标 |
| --- | --- | --- |
| `raw_dead_storage_and_reactivation_cannot_hide_a_live_reference` | 从实际 BorrowLocal 定位捕获根；提前结束模式移走原末尾同根 StorageDead；重启模式以一次合法 Dead→Live 过渡取代重复 Live，并保留末尾关闭 | Analyze 成功后 Verify 仍精确报 `E-BORROW-CONFLICT`。重启模式最早拒点可为 Dead：证明存活捕获不能跨 activation，不声称单独到达 Live 诊断 |
| `enum029_12_explicit_trap_is_ct_error_and_unreachable_still_rejects` | 将 Return 改成 Trap 前移除只对正常 Return 作承诺的 ScopeExit(Return)，保留根进入；Trap 为 abort，不伪造正常清理出口 | 合法显式 Trap 经 CT 验证后报 `E-CT-TRAP`；改成 Unreachable 仍报 `E-VERIFY-UNREACHABLE` |
| `raw_exclusive_ssa_transfer_is_single_use_even_without_source_local` | 按原 Store 的目标声明生成新局部；为新局部加入配对 StorageLive/Dead，包住原样复制的 Store | 第二个 Store 仍复用同一 SSA carrier，保持双消费攻击，精确报 `E-VERIFY-MOVE` |
| `raw_cfg_joins_duplicate_regions_and_dead_calls_cannot_gain_authority` | 按实际 UnsafeEnter 定位重复区域的插入点，置于现有 root/scope 内 | 原 CFG 合流、重复 unsafe region、死块外部调用三个反例仍精确报 `E-VERIFY-UNSAFE` |
| `raw_duplicate_readcapture_cannot_forge_independent_qualification` | 按实际 ReadCapture/WriteAddress/ScopeExit 定位，重复捕获和追加读取留在有效根内，读取仍跨过原写入 | 独立 Verify 仍精确报 `E-BORROW-CONFLICT`，不以 cleanup 结构错误代替 |

## 限定验证与交付

使用独立 worktree、独立 target、单 job、关闭 debug 与 incremental。
仅运行 `zil-driver` 上述五个 test target 的完整套件；保留全部原测试与诊断断言。
原始首轮正式失败、补充命令/退出码/日志哈希、受测精确提交及五项回归对应关系另交结构化收据。
施工者不运行正式全量，由另一作者独审补充后交 root 合入并重跑正式门。

## Rust 四目标边界

此补充恢复旧反例在新结构合同下的有效性，不新增语言能力或生产优化。
安全证据保留原借用/移动/unsafe/Trap 拒绝目标；运行性能、内存、表达维护成本与编译成本
均没有新的比较测量，不能将测试恢复记作对 Rust 的突破。
