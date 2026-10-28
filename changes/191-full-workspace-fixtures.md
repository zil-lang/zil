# 全仓旧边界夹具校准

生产冻结ae3ac244（同c860）实际全仓1890/8/0，199组，失败原件见run-logs/191-integration/formal-workspace02。此改动仅更新测试，不改生产/默认预算/输入。

Analyze三个精确budget夹具补188新的独立依赖表每函数6单位（4槽+frontier+checked槽），N-1仍必须ResourceLimit。042伪造地址签名仍保留原i32 entry，改为精确SafeReject E-VERIFY-CLOSURE-ENTRY。shared_addresses中两个原未调用lambda分别由189显式&Copy参数、184环境借出开放，原字面输入保留并执行native0；其它权限和借址负例保留。091/099原没有lending_captures的返回窗口仍被拒绝，按184精确E-VERIFY-LENDING。095原Package藏Desc继续拒绝，原无操作ByteBuffer Desc捕获按187准入，直接raw/Wire双路验证。

这些测试迁移只反映已实际合入的有限入口，不赋予额外来源、权限、Drop效果或slice参数能力。193的后续显式slice入口另行验收。
