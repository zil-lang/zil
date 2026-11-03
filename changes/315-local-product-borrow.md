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

315 已由集成者在主树 `_zil_design.md` §8.1.2.1 登记有限合同。后续按授权保留
`fn bad(p:&mut Root){}fn main(){}` 原字节，将它从共享切片旧非 Success 预期
移为 `original_mutable_product_signature_promoted_by_315` 显式正控；另外三个
借入返回/拥有 child 读取/拥有资源读取反例不改。共享分支原诊断文字继续保留。

最终本分支限定结果（LLVM 23.1.4，`llvm-api`，同一 target315/jobs1）：

```json
{
  "impl238_product_replacement": {"passed": 14, "failed": 0, "new315_tests": 7},
  "impl224_shared_product": {"passed": 13, "failed": 0},
  "verify_package_tests_shared_product": {"selected_passed": 5, "failed": 0},
  "impl235_product_store": {"passed": 9, "failed": 0},
  "new315_native_executions": 14,
  "new315_source_rejections": 16,
  "new315_raw_wire_rejections": 22,
  "new315_phase_budgets": {"types": 364, "analyze": 5022, "verify": 10661},
  "new315_each_budget": "N Success; N-1 ResourceLimit; fresh N Success",
  "targeted_clippy_D_warnings": "Pass",
  "diff_check": "Pass",
  "formal_full_suite": "NotRun",
  "native_windows": "NotRun",
  "product_comparison": "NotRun"
}
```

命令为前述两个 driver suite（合并为一次 `--test impl238_product_replacement
--test impl224_shared_product`）、`cargo test -p zil-verify
package_tests::shared_product -- --nocapture`（只计 5 个实际命中，其他 0 命中
测试程序不计通过）、`cargo test -p zil-driver --features llvm-api --test
impl235_product_store -- --nocapture`，及 `cargo clippy -p zil-driver --features
llvm-api --test impl238_product_replacement --test impl224_shared_product -- -D warnings`。

raw 窗口探针保留阶段事实：Analyze 可发布未核 CandidatePlan；普通 strong-loan
冲突由 Verify 独立重建并拒绝。将 DropPackage、字段 take 或第二个独占窗口移至
首次调用前，raw 与 Wire 均精确 `E-BORROW-CONFLICT`；未把候选产生计为安全通过。
借入前缺 child、移空/清理/失效 shell 则由 Analyze 与 Verify 分别拒绝。最后一次
测试修改未变生产代码，未增加额度、关闭门或改 lower/backend/schema。
