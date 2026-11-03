# 大数组后端来源与当前边界

`large-array-a18.zil` 原样保留262144字节嵌套数组，预期退出0。历史O0崩溃及O2结果仍保存在 [原始精简复现](../../../docs/validation/recovery-large-array.json)，不覆盖历史。

324候选 `5825cd582ac6b419d185b59486c7b7fc79f1bedb` 已通过恢复后的显式API feature复验：LLVM/Clang23.1.4，clang/API ×O0/O2四组均backend匹配、无fallback、原生退出0。普通大Copy值以独立栈快照及有界内存复制降低；不缩输入、不改请求优化等级。限定结果、此前默认feature跳过API的误报与更正见 [324变更记录](../../../changes/324-large-aggregate-lowering.md)。既有aggregates目标已覆盖，正式组合全量仍由集成方裁决。

目录名保留为历史路径。`large-array-a18-by-value.zil` 是后续一般巨型按值调用/返回ABI探针，尚未执行，不能声称已复现或修复。当前修复不等同于完整大值ABI与所有特殊消费者支持。
