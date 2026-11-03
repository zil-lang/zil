# 315：普通完整 Product 的局部独占借入

本次从现存 299 基线接续，不沿用未恢复的 301–312 源码或成绩。目标为
`tests/language-readiness/pending/mutable-product-helper-gap.zil` 原源。

拟沿现有 `Address`、`BorrowLocalMut`、字段地址与普通调用表示，开放完整普通
Product 的同步局部 `&mut Product` 及 helper 参数。建立借入前两核各自重建实际
局部完整性；地址及投影继续保留实际根来源，参数根与局部根保持区别。沿既有
NLL、父子重借和冲突检查拒绝活动窗口中的重借冲突、移动、替换及清理；只开放
已有 Copy 字段写入，不借此开放拥有字段写入、借入返回、长期捕获或异步。

依据为 ZIL-ORIGIN、ZIL-ACCESS、ZIL-CONTRACT、ZIL-PACKAGE、ZIL-CLEANUP
及 V02/V05/V07/V12/V13/V17/V22。§8.1.2 的原共享切片不被倒称为已支持独占。
本次是普通强独占窗口的新增有限实现，保持既有 Product 形状、独立 Verify、
schema/Wire、ABI、默认额度与产品门。若现有公共事实或机器路径不足，报告集成者
协调，不通过删除安全检查来准入。

限定验证将覆盖原源、局部/参数实际来源、完整/部分状态、窗口活跃/末用、
共享/独占相交、替换与失效，以及 source-derived raw/Wire 篡改；实际机器检查
与仅源码/IR结果分别记录。完整语言、life 正规化与 Self 原成果未恢复，均不作完成声明。

首轮限定检查：`source /workspace/.zil-env/env.sh` 后设
`CARGO_TARGET_DIR=/workspace/.zil-env/target315 CARGO_BUILD_JOBS=1`，运行
`cargo test -p zil-driver --features llvm-api --test impl238_product_replacement -- --nocapture`，
12/12 通过；新增原源 LLVM API O0/O2、父子重借/不同实际根、正常/错误清理、
源码最近反例与独立 raw/Wire 完整性/来源/权限均执行。初次 raw 断言误把移空、
清理及失效 shell 的首个拒绝阶段写为资源/借用检查；实际由独立初始化检查先拒，
现精确匹配 `E-VERIFY-INITIALIZATION`；缺 child 保持 `E-VERIFY-RESOURCE`。

`cargo test -p zil-driver --features llvm-api --test impl224_shared_product -- --nocapture`
首轮 11/12：未用 `fn bad(p:&mut Root){}` 原共享切片的 Unsupported 预期因新增
315 准入而失败，待保留字节迁移为新签名正控。原不可变局部 raw `mutable` 攻击
仍拒，精确诊断由旧 `E-UNSUPPORTED`/shared synchronous 迁移为
`E-VERIFY-MUTABILITY`/exclusive borrow requires mutable storage；此迁移经集成者授权。

本片有限安全及普通库组合有实证；原地地址/内联机器路径未增加隐式分配。
未作 Rust 配对运行/内存、维护工作量或编译成本比较，未建立产品级四目标或突破。
Store/Closure 等新形状、可变 child 深投影、可变捕获、返回/长期/异步仍未完成。
