# 205 Types 作者边界与旧断言迁移

本记录描述实际执行；规范仍为 `9813f43ee7c86d786f197f0ed4b8e65938ee354d94b2cdaec858e09e7871ce5e`，schema/Wire27 未改。原失败与旧规范身份不回填。

## 原 138 字节 State + replacement

原源 SHA256 `5251f83be296a6273532690540cfde459938f39801afc614ba1cbb672df2b05a`，138 字节。作者 baseline02 在 clean `11b3d825e8f0e9907b097c7e7227acffedfa16c2` 上记录 Expand / Unsupported / E-UNSUPPORTED / Verify0。该输入此前位于 `impl200_state_source::uncalled_definitions_and_closed_owning_shapes_do_not_gain_permissions`。

205 组合 clean `96bd747dbd2ef50e915b42d495ca21e1faca3132` 的 old200-before01 实际 0/1：同源已为 Verify / Success / Verify1，旧 Unsupported 断言失败。仅将该原字面移入 `uncalled_state_and_replacement_is_accepted_since_205`；old200-after01 在上述 HEAD + 此测试补丁上 `uncalled_` 三组 3/0。原 two-states、borrowed-inputs、重复三字段、Copy 算术、借入环境与 legacy 三字段 Types-only 控制均保留。

该原程序未调用 replacement 函数，不能据其 Verify1 声称函数已执行。实际调用、两个字段与独立环境、反序非相邻参数、FnOnce 补回及失败清理另在 `impl205_source` 中有真实 O0/O2 运行。

## 旧单 State 原始 Core 的未调用别名漏检

独审原始输入与测试来自 `71a3b24a12788a87a9f81c30ae1fcc7af2a4e5bb`。作者在 clean `bc8726b78346214f5359b73a415ddf36b3c77762` 的 legacy-raw-before01 原样复现 0/1：两个 Types 入口均接受 take/refill 后继续用旧 view 的未调用正文；Typed 仅有 main，未执行坏程序。

生产补片 `58937526` 仅在 `state_origins.rs` 把旧单 bound、真实 Package 参数纳入已有有界 Shape + Position/epoch 检查。分类和快照前付完整输入工作量，不生成新的公共 state_parameters，不将两个同 F 字段合并成一个运行值。legacy-raw-after01 的原测试 1/0；legacy-source01 的无别名双 FnOnce 字段、同一 exclusive view 重复使用与 sibling、旧双字段 lending 共 3/0、6 次 O0/O2 native。以上 after 运行实际为 bc8726b7 + 保存的生产补丁，不能倒标为 clean589。

## 实际 Types 工作量

同一个旧197 State 源及 Core 的字节在 budget-before01 与 budget-after01 一致。修复前 Types 最小 N=2642，补定义检查后 N=5293。普通 eager 泛型控制 N=88 和新 replacement 控制 N=4498 均保持。每个输入实际测试 N 成功、N−1 ResourceLimit、fresh N 与第一次完整 Typed 输出相等；最终测试另保留原 N=2642 的真实 ResourceLimit 记录。新增工作不靠跳过检查、退费或增大默认 limits 消除。

源 SHA256：`7cedc0a7d37056c9a4dea902c0e5475768296b83cdb8e9c91c6f3492e0fcb251`；Core SHA256：`0c7177dc9349c9ff37aa6d5ef6f8c6a39a1c561e60cd960cf39df3e101430adf`。这些是 Types 预算，不能与 Expand 的历史 N1731 混用。

## 其它原失败保留

source01 的原生首轮因 LLVM 动态库搜索路径缺失停于 Link，Verify 已过但 Native NotRun；source02 两项作者新 oracle 错把非泛型 main 的实际 Fn 当 FnOnce，真实输出保留，后改为显式 FnOnce finish 声明控制。source04 的 RHS `return` 表达式不在当前语法内，Parse 拒绝完整保留；后续具名源分别验证语句层提前返回与具体 E!i32 调用者中真正的 fallible RHS，不开放 E!State 模板。clippy-types01 缺工具 PATH、clippy-tests01 的测试 needless_as_bytes 均保留，随后按实际修复重跑。

运行原件位于外置 `run-logs/205-types/<run>` 及 `<run>-cases`；源码交付不收入执行产物，最终证据另包。
