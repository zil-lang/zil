# 质量探索

`zil qc` 是开发质量入口；Roam 状态探索和独立语义模型不进入语言关键词、运行时或生产安全核。语义 oracle 与生产 parser、types、analyze、verify、lower、LLVM 分开实现；有限整数模型用精确整数，真实 ZIL Source 经 CLI 编译并执行。模型通过只证明登记域，对独立 Verify 与机器行为没有替代权。

探索按有限 profile 冻结源码、工具身份、动作/seed/property、停止条件和预算。状态键保留编辑、取消与缓存历史；同一最终源码不必代表同一状态。记录首次发现、原始失败和缩减路径；注错、独审发现不能冒充自然发现。invalid、error、unsupported、incomplete 和预算耗尽均不算成功。原始运行在 `out/runs/`，goal 仅保存定位与决策。
