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
原 driver `impl192_copy_fields` 未改源码/判据，21/21（含原生正例）通过。
首轮手构测试遗漏 Scope、随后 whole-parent 负例未恢复 owning 槽而
导致双清理，均先报 IR 错误；已修正完整 Scope 和 Take+Store 责任，
不把上述前置失败当借用核证据。

Rust 四目标：只恢复实际窗口精度，沿原内联表示且无新增运行时机制；
没有 Rust 运行/RSS、维护或编译性能配对。完整安全及产品四目标合取未证，
本片不改变 B0、完整语言、产品或正式全量状态。


费用以同一真实 raw/Wire 反序列化模块测得；旧值来自冻结 c1 构建的
独立 Analyze 入口，未将早拒数当成功费用：

| 输入 | c1 旧结果 / N | 修复后结果 / N |
| --- | --- | --- |
| 不同字段、当前 callback 参数 | Borrow 拒绝 / 2202 | 接受 / 2387 |
| 最后调用后同字段写（首调用实参是 sibling） | Borrow 误拒 / 2353 | 接受 / 2560 |
| 相同字段、当前 callback 参数 | Borrow 拒绝 / 2202 | Borrow 拒绝 / 2202 |
| 原标量捕获、最后使用后写、无字段借入 | 接受 / 1291 | 接受 / 1291 |

增量来自避免错误短路后实际继续核验调用、窗口、责任与导出工作；
没有跳过检查、减免费用或提高默认限额。持久测试固定上述新 N，
raw/Wire 均复验 N、N−1 和重新请求同 N；公共字段地址 Hold 仍是
原 parent Local。临时逐项探针位于 `/tmp/zil-288-budget`，非产品证据。
