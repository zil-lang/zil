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

## 223 历史 oracle 的 315 有限迁移提案

root 6907974 的正式默认/参考首轮暴露三项旧期待：impl223 的 coherent Root 地址
改写、review223 的同输入及冻结 schema32 replay。它们均保留实际 incoming
`&mut Root`、Parameter(0) Hold，只写外层 index1 的 i32 Copy 字段；Root 为
递归 Managed<R Copy 头> child 加 Copy 字段，无 Store/Closure/拥有 Sum/数组。
这属于新增 §8.1.2.1 同步 helper 合同，并非 deep child 写入或从只读来源制造权限。
参数 local 的 immutable 仅限制 pointer 绑定重赋值，不撤回实际 incoming &mut。

经 root 授权，仅迁移三个测试当前合法 source/raw/Wire 为显式 315 正控；保留全部
原源码、ORACLE.json、schema29/30 JSON、Wire27/29/30/32 fixture 原字节与旧传输
版本拒绝。不改生产。每个迁移入口补 shared 地址仍写、owned child 目标和调用时
缺 child 的最近反例，固定真实 phase/code，不能以空选择、fingerprint 或资源耗尽
代替语义拒绝。历史旧 Unsupported/UnexpectedAccept 记录不重标为当时通过。

迁移限定命令沿用 target315/jobs1：`cargo test -p zil-driver --features llvm-api
--test impl223_product_store --test review223_boundaries --test review223_replay
-- --nocapture`，分别 13/13、2/2、2/2，无过滤。三个迁移入口各自保留 coherent
raw/Wire 正控，再各核 shared-write 的 Verify/`E-VERIFY-ADDRESS`、owned-child
目标的 Verify/`E-UNSUPPORTED`（不准形成 owning child 可变地址）、以及真实调用
缺 child 的 Analyze/SafeReject/`E-ANALYZE-IR`；共 12 个 raw/Wire 近邻拒绝及
3 个真实源拒绝，均绑定精确 phase/code。所有 fresh fingerprint 重绑定保留。

全部原 fixture 的 diff 为空。原 `mutable_control.zil` SHA256 为
`b3f19abd09088e6686ebb12eb50a0058429ba38a8568274c789050160716f75a`；
原 ORACLE.json 为 `d4e9dcc8e54b2a3029e9d9820e0ccfe65885a7c19f22eef7193ff00a263ecb94`。
现 schema32 的 1744 字节 SHA256 新增显式断言为
`8cc0e4a565e57b71135b3c692079dcaa65dfa4ce6f49dbe36c253182df497862`；
依旧逐字核对仅版本 envelope/fingerprint 相对原 schema27 的允许范围。
Wire27/29/30 的既有固定 SHA、decode/Verify `E-WIRE-VERSION` 拒绝均不改。
三个测试保留实际 source/raw/Wire/JSON 输出；迁移只改测试与本记录，无生产变更。
