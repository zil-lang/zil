# 325 调用位置与保守资格扫描澄清

有限结构化线程调用仅在普通 Direct 函数内实现。父闭包入口或带 callable 合同的 helper 内调用暂报 Unsupported；这些位置仍是终态待实现义务，不把父调用位置混为 worker 的安全反例。worker 静态调用图逐函数扫描全部 CFG 块，不以源常量分支隐藏不支持的效果。

最近对照：普通函数中 body 合法捕获父线程借用仍可执行；闭包内部调用返回 Unsupported；普通 worker 调用图内存在 FFI 即使位于常量未走分支也不获得线程资格。Types、Analyze、Verify 分别实现入口与资格检查，公共 schema/Wire34 不变。
