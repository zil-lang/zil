# 194/195 合流后的既有测试预期迁移

主线 `7d8cea72460d059f76987f076afa147ed68184eb` 在独立空 TMPDIR 的正式全仓执行为 **2059 通过、4 失败、0 忽略**。实际命令与原始输出保留在 `run-logs/195-integration/formal-workspace01`；该次结果保持 Fail。

四项均由原输入与当前设计逐项核对，`/root/resume188_acceptance` 另作只读核验。此次只迁移测试期待，不改生产、源输入、原始 raw 构造、预算、规范或产品门：

1. driver `generics` 的第三个原输入是未使用的合法 `T: Copy` identity。194 已实现结构 Copy，改断言为 Success / Verify / verification_runs=1；前两个泛型参数数目和不同整数类型的反例仍拒绝。
2. `impl147_static_traits` 的 Copy / Share / Transfer 三个原 format 输入全部保持 SafeReject。Copy 现在由 Expand 更早给出 `E-GENERIC-ARGUMENT`、`Copy is a reserved structural capability name`；Share / Transfer 的 `E-TRAIT-RESERVED` 不变。保留零 Verify 执行。
3. Expand `generics` 的未使用 Copy bound 原输入合法，核展开成功、无 unsupported、仅实际 sample 函数和有效源映射；未声明 MissingRead、把 struct Read 当 trait 的两个反例继续核原诊断与精确跨度。
4. Verify `helper_tests` 的原 raw 仍为真实可达自环却声称 Finite。190 已实现 Flow，独立重建后拒绝错误合同；原 raw / Wire 两路均须精确 SafeReject / Verify / `E-VERIFY-CALLABLE-CONTRACT` 和 traces 不一致诊断，不再期待旧“全部循环未实现”的文字。

限定复跑与更新后的正式全仓必须另记实际提交与结果，不能把这份预期迁移说明当作通过。完整语言 NotSatisfied、产品 NotRun、B0 Doing 保持。
