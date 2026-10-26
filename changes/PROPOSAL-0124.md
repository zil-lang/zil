# 124：补齐有符号位移，驱动普通 UTF-8 文本代码

## 原因与范围

基线 `ff0ddb7e59b2b0577c35b92e74b895c86f20b218` 上，普通 UTF-8 decoder 的 `scalar:i64; scalar=(scalar<<6)|(byte&63)` 经 Types、Analyze 后被 Verify 拒绝。相同代码将移位改成乘64即可通过，说明本片其余前置已具备。编译期整数 VM 已有 i32/i64 的 checked_shl/checked_shr 实现，但 source `#(...)` 同样受 Verify 的旧 u8/u16 准入门阻挡；不能把这一实现事实写成基线已支持 signed CT。

本片仅把现有二元位移运算贯通到 i32/i64 的独立核验、CT及LLVM。没有新增类型、指令或wire标签，schema仍为17；不引入字符串内建、自动分配、RC、地址或FFI权限。UTF-8验证、字节位置与标量索引由普通ZIL模块完成，是ZUI文本输入需要的库能力。

## 明确语义与安全推导

两个操作数必须是同一种现有整数类型，结果仍为该类型。位宽 w 分别为8、16、32、64；移位量必须满足 `0 <= n < w`。非法量在CT产生既有 `E-CT-ARITHMETIC`，运行期沿既有checked trap路径失败。移位量不按机器指令隐式取模。

左移保留低 w 位，按该类型解释结果；这与已有u8/u16和整数CT宏的位操作语义相同，不把位移改成checked乘法。i32/i64右移以符号位填充；u8/u16右移仍填零。负的左操作数合法。相同位宽的有符号移位量按无符号比较 `n >= w`，负数的二进制表示一定位于无符号区间 `[2^(w-1),2^w)`，因此同一guard同时排除负量与超宽量。

LLVM在guard成功分支发射不带`nsw/nuw/exact`的`shl/ashr/lshr`。合法分支只含小于位宽的移位量，故不依赖LLVM对超宽量产生的poison，也不把符号位变化误加成溢出假设。右移的有符号/无符号选择由已独立核验的实际operand类型决定。

依据：LLVM Language Reference 的 [shl](https://llvm.org/docs/LangRef.html#shl-instruction)、[lshr](https://llvm.org/docs/LangRef.html#lshr-instruction)、[ashr](https://llvm.org/docs/LangRef.html#ashr-instruction)；编译期对应实现是本仓 `crates/zil-ct/src/lib.rs` 的整数宏。此推导只说明本片操作的语义与机器边界，不构成完整语言安全或性能证明。

## 接受性迁移与验证

root已授权迁移旧`impl082_bytes_ffi`的signed-shift暂不支持期待：合法i32/i64移位现在接受；原u8运算覆盖继续保留，非整数与不同类型的raw/wire操作仍按旧诊断类别拒绝。新测试分别观察CT合法结果、CT负/等宽/超宽拒绝、O0/O2动态合法位模式与精确SIGILL失败，不以任意崩溃作为通过。

新增真实源码消费者至少跨三个模块，使用普通UTF-8 decoder与文本输入统计，覆盖1–4字节、NUL、最高标量、overlong、surrogate、超上限、截断和非法continuation；逐标量扫描与字节位置由实际运行结果检查。编译器、库、原始IR/wire和原生执行的证据等级分开记录；第一次夹具错误及基线拒绝原件保留。

默认预算、规范摘要、产品门、独立Verify边界均不修改。旧接口文件的u8有限profile是其历史增量范围，本片不改写其旧结果。`bytes_truncate`为后续独立存储需求，不进入本单。
