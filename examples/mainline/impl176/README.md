# 具体回调 State 字段

这些源文件直接取自作者最终通过的机器测试：`main.zil` / `model.zil` 是跨模块 `pack<F>`、`tick<G>` 与 `twice<H>`；`field_lifecycle.zil` 是嵌套资源闭包取出、调用、补回与清理；`failed_replacement.zil` 用错误返回证明 RHS 失败不会提前清理旧 callback 字段。后一个程序按合同退出 1，输出 `TA`。完整重放使用仓库 `tools/language/176/check.sh`，所有测试都通过通常的源编译与独立 Verify。
