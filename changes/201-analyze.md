# 201 Analyze：有限 callback Store 元素与实际父来源

本片作者为 `/root/bundle_sync`。它是 201 的 Analyze 组件交付，不是自审收据、201 全仓验收或 1.4 终态声明。

规范绑定：design `4c57f670cae6805444e0531485deee9c3a4a9486f533fc97e49f0b5104bccfbf`，normative `93055a7cf5c5692a9e3fbf55b73c82743c51e48d582537504b9dfaeaf29cf09c`，schema 25。从正式派工 `0b41f02af45d3ee834b3b6327e0fb444ce659183` 建立 `impl/201-analyze`。

## 实现及独立边界

生产提交 `064f96cf318b1a9121a3591bc4b64a3246d41439` 仅修改 `zil-analyze/src/dynamic_store.rs`、`budget.rs`、`resources.rs`：

- 原有 preflight 类型遍历携带最外层 Store 元素来源；只有在该元素树内实际见到 `Type::Closure` 才预费并记录新资格检查。覆盖参数、返回、SSA、错误载荷、资源定义、捕获类型和 `OwnedStoreNew` 的裸元素元数据。普通非 callback Store 没有第二次扫描或新增逻辑费用。
- 全部输入类型和捕获树的结构限额完成后，再检查实际元素：完整 Package、一个或两个同 ClosureId 的直接 callback 字段、至少一个直接 Managed 资源，其余为有限 Copy 或允许的直接 Managed。隐藏在地址、数组、嵌套 Package 或 Store 内的 callback 不能充当直接字段。
- 从实际 Closure 定义重走现有自含捕获检查。直接 Store、含 Store 的 Package、外部 Address/Desc 和嵌套 callback Package 捕获仍关闭。先核元素/捕获，再生成候选指纹；不把 Types 的分类当成证明。
- `CallClosureBorrowed` 在资源分析中明确拒绝 FnOnce，以及缺独占地址的 FnMut。拥有位置的原 FnOnce 消费、完整 push 转交、失败错误载荷、pop 转交、剩余前缀清理继续走原有责任分析。

现有 Hold 分析已经沿 Store 描述符、元素窗口、`AddressField`、`ReborrowAddress` 和 lending 调用结果传播同一个父 Store Local。本片没有修改 `holds.rs`、添加公共候选事实或改动 schema。手写 IR 和实际源码派生均核对这条来源链；借用冲突的独立 Verify 检查由 root 的实现及非作者审核承担。

Analyze 检查有限形状和实际环境责任，不新增 ABI 布局算法。真实非零 stride、闭包标志字节、目标溢出和布局环由 Verify 独立核定，LLVM 独立计算实际表示。不得把 Analyze 产出候选等同于通过这些布局检查。

## 实际执行与源码身份

所有列出的运行均在各自干净提交上执行；命令、输入、输出和源提交保存在独立证据归档。不同运行没有倒改为后续提交身份。

| 运行 | 精确提交 | 实际结果 |
|---|---|---|
| 原普通 Store 预算基线 | `cbaf40b936bf54a7f9b7d0ad9b3ce7c59e39231a` | 生产未编辑；1/0，Analyze N=561 |
| 201 手写 raw IR | `064f96cf318b1a9121a3591bc4b64a3246d41439` | `impl201_store` 9/0 |
| 有限相关回归 | 同 `064f96cf` | budgets 12/0、roles196 10/0、constructor200 8/0、package_budgets 5/0 |
| driver 测试编译 | `3017a148a8dbd40f7b03d2817e337da540ea4714` | `--no-run` 成功；不计源运行或 native 成绩 |
| 来源派生首轮 | `8386cbbcd37f5eb03d688fe7070d7f220bbb90ad` | 6/0；已合 frontend `462f7e6a`、Verify/LLVM `63a90ad9` |
| 来源派生收紧轮 | `175126856d7d78085c0ca0547833db1925b1f279` | `impl201_analyze_store` 7/0；生产与前轮相同，仅收紧测试 |

最终来源组包含每条 raw/Wire 路线 5 个正向对照、4 个 SafeReject 攻击和 1 个 Unsupported 资格攻击。原 02 与原 Pair 拆两 Item 的输入都到达实际 Analyze、Verify 和 Wire；后者包含两个独立 push、两个 pop 和逐元素 callback 调用。另有实际 FnMut 与 FnMut lending 字段窗口，两者候选中的 callee Hold 和 lending 结果 Hold 均为父 Store Local。

精确拒绝分层：

| 攻击 | Analyze | raw / Wire Verify |
|---|---|---|
| 第二次 push 重复第一个实际 SSA 元素 | SafeReject `E-ANALYZE-IR`，已消费责任 | SafeReject `E-VERIFY-RESOURCE` |
| 删除父 Store 前缀清理 | SafeReject `E-ANALYZE-IR`，存储边界丢弃拥有责任 | SafeReject `E-VERIFY-RESOURCE` |
| 把实际拥有捕获伪造成 Desc | Unsupported `E-UNSUPPORTED`，自含资格 | Unsupported `E-UNSUPPORTED` |
| 借入环境强行 FnOnce | SafeReject `E-ANALYZE-IR` | SafeReject `E-VERIFY-CLOSURE-CALL` |
| 实际共享重借后强行 FnMut | SafeReject `E-ANALYZE-IR` | SafeReject `E-VERIFY-CLOSURE-CALL` |

首轮共享 FnMut 攻击只修改结果类型，Verify 因 `E-VERIFY-TYPE` 提前拒绝。该原记录保留，不能当作权限门证据。收紧轮同时把实际 `ReborrowAddress.mutable` 改为 false，类型一致的共享重借才到达上述调用权限门。

## 预算

所有 N 都是 Analyze 的逻辑步数，未声称其余阶段具有同一 N。每项实际验证 N 成功、N−1 单个 Analyze ResourceLimit、fresh 默认调用及 exact 调用生成相同候选；默认限额未提高。

| 输入 | 精确 N |
|---|---:|
| 原普通非 callback Store | 561，前后相同 |
| 手写单 callback 元素 | 772 |
| 手写双 callback、窄 Copy 捕获 | 1031 |
| 手写双 callback、宽 Copy 捕获 | 2023 |
| 手写 FnMut lending 窗口 | 1613 |
| 原 02 源码派生 | 14578 |
| 原 Pair 拆两个 Item 源码派生 | 51708 |
| 实际 FnMut lending Store 窗口源码派生 | 15633 |

旧 Analyze 精确预算、196 角色预算和 200 构造预算也保持原断言。新字段、类型和实际捕获工作在执行前计费。

## 三份原输入与未执行范围

实际逐字比较 preflight Git 原件，结果：原 02 为 626 字节，SHA256 `23faee6a4575e6c522fa8ffed71d929dfd8463241197c7760cbe670f11f65fa8`；拆 Pair 为 1157 字节，SHA256 `003e862d7741c0a7cdfc47b6263860c73641c912d69ec7a55a23d9840eeea5bf`；Package 捕获 Store 为 484 字节，SHA256 `5d430f79813e995268368bcbe3c3918e1de093007e5802f79d4f88f196905853`。

第三份原输入继续在真实 Types 阶段以 `E-UNSUPPORTED` / `Package capture with Store fields requires a retained storage protocol` 拒绝。该源输入的 Analyze、Verify、native 为 NotRun，不能计作借用或清理安全验证。

本组件没有执行 native、Win64/Wine 或原生 Windows，也没有执行正式全仓；这些结果由其实际执行方分别登记。小收据入 Git，原始 source/IR/Wire/命令/日志单独打包，归档不包含编译产物或可执行程序。
