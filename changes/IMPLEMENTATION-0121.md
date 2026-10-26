# 0121 — 跨模块名义类型进入 callable 接口

本单在恢复后的 draft22 上重新实施，没有恢复或复用丢失的旧116提交。生产改变只有 `zil-expand/src/generics.rs` 的23行：已支持的 Fn/FnMut/FnOnce helper 保留 callable 外层关键字，内部参数/结果类型沿同一有界实例化器访问。别名透明性、模块所有者、实例键、后续类型/责任/独立核验规则保持。

## 实际效果

原基线 ed33deba 的三模块 record/Fn 正例在 Types 以 E-UNSUPPORTED 拒绝，Verify未执行。候选249331b7及最终受测8410e863把 record/enum 的实际值传入并返回 callable，六种程序各在O0/O2执行，12次均输出42。FnMut实际读取捕获计数器，FnOnce实际消费拥有Token；值未在调用helper之前归一成整数。

错误泛型实参和同名但不同模块所有者的闭包参数各以 Types/E-TYPE-MISMATCH拒绝。by-value FnMut、借用FnOnce和递归签名三个既有边界没有放宽。独立Verify分别接受原始raw和wire候选；把名义参数伪造为bool后，两条入口均以 E-VERIFY-CLOSURE-ENTRY拒绝。

## 有限验收

最终在8410e863执行44个测试组：本单5、既有helper17、alias13、generic regressions9；全部通过。fmt和指定test target的clippy通过。正式全量仍由集成者安排，作者不自审合入。

新增类型遍历按实际工作收费。固定源的工作边界862→886、节点277→289，深度8不变；新N产物与默认完整Core相同，N−1明确ResourceLimit。默认额度未调高，旧低估成本的接受边界不声称等价。

## 证据与四目标

`evidence/impl/121/acceptance.json`绑定源码、计数和完整原始归档；旧失败、首次候选、最终近邻测试、源码/阶段输出、机器结果、raw/wire反例全部保留。原始归档1,755,493字节，包含1,708个有效载荷文件；不包含Rust构建缓存。

本片让用户可在既有公共helper上直接组合名义API，不必手工把类型拆成标量。安全结论限于当前静态Copy record/enum与同步调用合同；没有共享前端证明给Verify。ABI与运行表示未改，但没有相对Rust的运行/内存测量；新增正确收费也不是编译提速。完整语言NotSatisfied、产品NotRun、B0 Doing及四目标整体待证保持。

