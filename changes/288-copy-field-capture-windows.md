# 288 Copy 字段捕获窗口精度

基线 `c1db412f48b8`。279 的普通捕获窗口把 `BorrowPackageField`
得到的普通 Copy 字段指针私下归成整个 Local，误拒原合法192源码：
`captured=&state.x` 的闭包调用接受 `&mut state.y`。同字段 `state.x`
与整父对象的 move/drop/replace 必须继续相交。

本片恢复既有字段分离语义：独立检查后的 `BorrowPackageField(local,index)`
在 Analyze 私有 provenance 中记为实际 `Field(local,index)`；公共
ValueHold 仍导出原 parent Local，不改 IR、Wire、默认预算或 Verify。
实际同字段、整父位置与 retained 捕获依赖继续按原 overlap 规则检查。
不按字段名、名义类型或 Verify 的结果授信。

限定验证在 target-288/jobs1。checkpoint 已完成独立 raw/Wire 同字段冲突、不同字段接受、最后使用后同字段写及整父移动冲突。
同字段拒绝 N2202、不同字段接受 N2387、最后使用后写 N2560，
raw/Wire 的 N 成功或精确安全拒绝、N−1 ResourceLimit、fresh 等价均通过。
后续复核原 driver192 源码原生控制与无字段旧控制，费用按真实工作计账；
首轮手构测试遗漏 Scope 而先拒，已补完整真实 Scope，不算安全核证据。

Rust 四目标：只恢复实际窗口精度，沿原内联表示且无新增运行时机制；
没有 Rust 运行/RSS、维护或编译性能配对。完整安全及产品四目标合取未证，
本片不改变 B0、完整语言、产品或正式全量状态。
