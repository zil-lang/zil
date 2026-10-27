# 175 独立审核：Pass

审核者为 `/root/callback_field176`，不是 175 施工作者。审核在独立 worktree
`review175-by176` 完成，绑定交付提交
`579d49a76ad2aaef8b1c007289255dd1c4c7a54f`、生产提交
`fd69ff6c7cbbc95fcd395d5dbe11fb3c3c85790d`，二者的 `crates` tree 均为
`6937ecf38b4bf300df400ec462772b60b30ea73a`。继承的 174 生产依赖为
`9a878f2f91aae4efd4b851eeb8979cacc21ffdd1`。

本审核没有修改生产代码。新增独立测试
`crates/zil-driver/tests/review175_by176.rs`，其精确字节哈希、环境和审查范围见
`review.json`。本片没有阻断发现；结论只覆盖当前有限合同，177 的 174/175/176
合成验收由集成者另外执行。

## 实际运行

| 检查 | 最后结果 |
|---|---|
| 作者的 `impl175_store_views` 限定重放 | 20/20，通过日志为 test02 |
| 新增非作者 `review175_by176` | 4/4，通过日志为 test04 |
| 独立原生执行 | 6 次正常退出、4 次 signal 4 trap，均覆盖 O0/O2 |
| 新增源码反例 | 4 个，均在 Verify 得到 `E-BORROW-CONFLICT` |
| 新增恶意候选 | 2 个，每个 raw 与 Wire 两入口均得到 `E-BORROW-CONFLICT` |

没有运行工作区全量。重复的 test03/test04 不累计成额外能力或额外测试数量。
作者测试的独立重放包括既有实际长度边界、字段可见性、清理、别名与精确
Verify 预算测试；本审核没有重跑作者的 50 次 CLI 分配消费者，也没有把作者
原生结果与非作者结果混合计数。

## 新增探针及其意义

审核采用与作者不同的元素布局：`Cell` 拥有一个资源，并包含 `[i32; 3]` 与
`i32` Copy 字段；`State` 拥有一个资源及两个 `Store<Cell>` 字段。

1. 从 `left` 元素取得数组成员地址，经普通 `id(&i32) -> &i32` helper 返回后
   读取。在 helper 中增长另一个 `right` Store 合法；最后使用结束后增长
   `left` 或移动完整 `State` 也合法。O0/O2 实际读取结果正确，退出清理输出
   均为 `ZAB`，对应 State 自有资源、left 元素、right 元素各一次。
2. 将同一 helper 的 Store 参数改成真实 `left`，或在子地址仍将使用时增长
   `left`、替换 `left`、移动整个 `State`，均产生真实 Verify 借用冲突。
   参数名、标量子地址与 helper 返回值没有掩盖实际来源。
3. `left` 的实际长度为 1、容量为 8。索引从真实 stdin 读取：`0` 正常，`1`
   与 `/`（转为 -1）在 O0/O2 均触发 signal 4。实际 stdin、编译结果和原生
   输出保留在各 case；没有用编译期常量替代运行时索引。
4. 第一个 raw 反例直接将 `OwnedStoreBorrow.store` 改成同一父壳 `right` 的
   描述 SSA 值，后面保持原有 right 写入；公共 Hold 粗根仍是同一个 State。
   重新计算 fingerprint 后，独立 Verify 仍重建精确字段并拒绝冲突。第二个
   raw 反例把完整父壳 `TakeLocal` 提前到借用之后、读取之前，保留实际后续
   读取；raw 与 Wire 两入口都拒绝，不能只靠前端的顺序或初始化检查。

## 生产差分阅读

审核阅读了 175 相对继承 174 的差分，以及相关独立 Verify 与 LLVM 实现：

- `OwnedStoreBorrow` 要求真实 `Desc<OwnedStore<Package>>` 和 i64 索引，结果
  是精确元素的共享 Address。独立元素准入继续核对 nominal Package、sealed
  Store 定义、非零布局及大小上限；没有开放通用 Package 地址。
- `AddressField` 对元素重新检查准入，结果字段须通过有限 Copy 类型核验。
  Managed 字段、直接元素视图捕获、可变元素地址与派生地址返回仍保持拒绝。
- loan 与 root 从实际指令重建。Store 字段保留 `Local + Field`，整父壳
  move/drop 与字段窗口相交；跨函数效果按实际实参来源实例化。
- LLVM 先读取 head 的实际 `len`，作无符号 `index < len` 比较，成功后才
  读取 data 指针并按实际元素布局计算地址。当前新增布局与动态输入探针
  在 O0/O2 验证了该路径。

## 失败记录和复现

全部四次命令与日志保留在 `commands/`，没有把审核夹具失败计成语言实现缺陷：

- test01 是审核测试错误导入不存在的 `zil_driver::compile`，Rust 编译失败，
  未执行测试；改为现有 `compile_source`。
- test02 中作者 20 项全过，新增审核 3/4 通过。剩下一项把 `unsafe` 语句写成
  了表达式，真实 Parse 拒绝原样存于 `failed-fixtures/`。
- test03 改成合法的 `unsafe { input = getchar() }` 语句后，4/4 通过。
- test04 只补实际 stdin 与 raw 原始源码的持久化，再次 4/4 通过；这是最终
  case 记录的来源，没有增加或放宽断言。

可在包含本审核测试的 checkout 中分别执行：

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test -p zil-driver --test impl175_store_views --offline -j2 -- --nocapture
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test -p zil-driver --test review175_by176 --offline -j2 -- --nocapture
```

设置独立的 `CARGO_TARGET_DIR` 与 `ZIL_TEST_REPORT_DIR` 可以隔离编译和输出。
实际运行使用的工具链、完整环境和原目录见 `commands/commands.json`；历史
绝对路径标识来源，不充当新环境的目录要求。

`case-index.json` 对应 16 个最终输出目录：10 个原生、4 个源码拒绝、2 个
raw/Wire。`field-helper-phases/` 保存一个实际正例完整阶段输出；
`runtime-boundary/llvm.ll` 保存真实动态边界例的机器表示。其余实际阶段的
字节哈希在 `all-generated-phase-hashes.json`，作者重放输出哈希另列于
`author-replay-output-index.json`。可执行文件只记哈希，不收入证据。

## Rust 四目标及未完成条件

| 目标 | 本次独审新增证据与限制 |
|---|---|
| 同类安全 | 真实根、参数别名、子窗口和整父壳失效经源码及 raw/Wire 核验；不是完整语言安全证明 |
| 运行与内存 | O0/O2 行为、边界 trap 与一次清理成立；未测相对 Rust 的运行时间或峰值内存 |
| 表达与维护 | 普通 State、Copy 数组字段与常规 helper 可以组合；未测维护工作量优势 |
| 编译成本 | 只做正确性限定运行；没有性能比较或编译速度结论 |
| 实质进展 | 动态拥有元素可经有限共享窗口读取，子字段地址仍受实际来源约束；不是产品净胜出结论 |

一般可变窗口、借用逃逸、混合保留捕获、更多元素形状及 Linux x64 之外的
Store 实现仍未完成。B0 `Doing`、完整语言 `NotSatisfied`、产品 `NotRun` 保持。
