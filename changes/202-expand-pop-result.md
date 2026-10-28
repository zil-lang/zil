# 202 Expand：普通 helper 成功载荷的调用上下文

冻结生产为 `f200d5be6a4af1facd90e95daf860464681ae714`，只新增 `generic_store::call_context` 的八行判断。独立复核原源先把两个完整 `Item<Left,F>` / `Item<Right,F>` 分别放入 Store，经普通 `pop<T>` 的成功分支取出，再把两个实际 callback 字段借给 `invoke<G>`。该源原先在 Expand 定义检查阶段错误地拒绝：外层 `void!i32` 被强行用来约束内部 `invoke` 的具体 `i32` 结果。

现在只有 callee 自己明确写出的 error union 结果，才用 caller 的整个 union 约束。普通结果载荷或尚未绑定的泛型结果由实际参数确定，最终仍经过原 `same_return` 检查。已有 Store 成功返回的明确上下文保留。此修复没有把返回类型不匹配改成忽略，也没有修改 Types、Analyze、Verify 或运行时责任检查。

原始独立源逐字保留在 `POPPED_HELPER_SOURCE`。干净 `pop04` 实际通过 17 项 Expand 检查；原源精确边界 12291，12290 返回 ResourceLimit，新的 12291 预算重新成功。旧 1731、2255、8399 和同名碰撞 12755 均保持。最近反例中，具体 `i32` 返回到 `void!bool` 仍由 `same_return` 的实际类型一致性检查拒绝；callee `bool!i32` 返回到 caller `void!i32` 仍因错误侧不匹配拒绝。匹配 `void!i32` 的整个 union 正对照成功。

`pop01` 的原失败、临时诊断打印的 `pop-debug01`、dirty 的 `pop02`/`pop03` 及 clean 的 `pop04` 分别保存。临时打印只为定位 `i32` 对 `void!i32` 的错误约束，未进入提交。本组证据是 Expand 层检查；五模块消费者在 Types 中丢失 error union 成功形状是另一处问题，由 Types 作者独立修复，不能用本收据替代最终 native 结果。

小收据为 `evidence/impl/202/expand-pop/SUMMARY.json`。完整原源、Core、来源映射、命令、输出和输入差异独立打包为 `202-expand-pop-evidence.zip`，不含执行产物。
