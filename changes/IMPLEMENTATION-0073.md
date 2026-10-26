# 073 package 字段状态与清理的独立核验

2026-10-24（Asia/Shanghai）。生产基线 `0c093f5e9fad117b8913d912020e48e9040d581a`；主树权威任务 `.task/impl/073.md`；在生产代码修改前记录本提案。遵循 `_zil_design.md` §§7–9、15.2 及 074 已冻结的三个字段原语，不调用 Types/Analyze 的证明实现。

## 需要补足的边界

原 Verify 只为完整 package 记一个责任位。允许局部字段移出、条件清理及重新填入后，单一责任位不能证明完整整体移动，也不能分辨兄弟字段的描述和旧实例描述。该片独立重建外壳活动、每个直接 Managed 字段责任以及真实描述 liveness；普通 package SSA 与函数 ABI 仍总是完整形状。

## 实施合同

- `TakePackageField { local, index }` 只接受活动 package 的确定 LIVE Managed 字段，消费字段责任并产生一个 INPUT 新鲜度的 Managed SSA。允许不可重绑定的 let 外壳移出字段；禁止通过该动作伪造 create 出口。
- `DropPackageField { local, index }` 条件清理目标字段当前实际 LIVE 的责任，保留外壳、兄弟字段及外壳的初始化次序。它用于源级替换的显式旧值清理，不能清理 Copy 字段。
- `StorePackageField { local, index, value }` 独立检查 local 存在、活动、可写、索引和精确类型。Managed 字段所有入边均无 LIVE 旧责任，才能消费 Managed SSA 并建立 LIVE；Copy 字段按精确 Copy 类型复制。
- 完整 Store 和参数入口建立全部拥有字段；whole Take 要求每个拥有字段确定 LIVE，消费全部字段。DropPackage 按声明顺序条件清理剩余责任，结束外壳；部分形状不能经 Call/Return/MakePackage 等 SSA 边界逃逸。
- 字段债务槽不加入局部初始化次序矩阵；它们仍参与返回泄漏及存储结束检查。字段槽分配、数据流复制/合并/扫描及描述根访问均计入现有预算。
- 描述根保持精确 `(local, index)`。字段变化只拒绝该字段仍被未来实际使用的 Desc/alias/capture；整体变化覆盖全部字段根。循环回边必须保留活的旧实例描述，不能因物理槽相同或新一轮初始化而忽略。
- 新指令参与 operand/SSA、初始化、局部存储 scope 和独立资源守恒重检；候选计划不可信。schema12 由 074 统一修改，073 不改变公共 IR 或产品门。

## 限定验证与反例

保留原完整 package 测试，新增手写 raw IR 与 wire 双入口反例：重复 take、从 DEAD 字段取 Desc、缺字段整体转交、LIVE 直接覆盖、重复 SSA 消费、错误 local/index/type/result、let 填入、同字段旧 Desc/捕获跨替换、StorageDead/return 遗债、不活动外壳、错误 create 归因、混合分支和循环状态。正例包括兄弟字段 Desc、部分包退出、条件 take 后 Drop+Store、循环恢复及完全恢复后的整体移动。所有原始案例只证明独立核验层，不冒充源码或 native 结果。

仅运行本片限定测试并保留失败证据；精确候选提交交非作者审核，root 执行正式全门与集成。RHS 准备失败、源级错误出口及机器清理次序由 072/074 的各自证据补全，本片不以 Verify 接受替代运行语义。

## 作者交付观察

- 公共接口消费 `96bf717ad36cc1b79455502ed65e4dccff15470d`（本支等效提交 `02016595f38698a008d8ec344d10d9f40bddd00e`）。schema12，规范身份 `972e3c9e79869f87fdeaeee63cc87c27e0f5f42af5312c309f695e823f980e3e`。
- 生产实现与新增 25 组测试提交 `ad7375b99e11cf30b3a3f7b9edd5af41b7b06718`。`package_tests` 限定运行 59/59 Pass，包含旧 34 组与新 25 组；`resource_` 限定回归 37/37 Pass。两个选择有重叠，不能相加当作独立测试数量。
- 字段条件 Drop 允许已有字段责任为 MOVED/DONE 的空操作；完整 DropPackage 保留重复自动清理拒绝。所有公开语义 fixture 同时通过 raw 与 wire 入口。另有明确标注的内部资源预算测量，检查字段状态真实收费及不扫描无关 nominal resource 表。
- 三次作者运行均成功，无需故障修复或缩减。未运行正式全门，未自审合入，未声称上述测试证明源码 prepare/commit 时序、LLVM 动态 flag 或 native Drop 顺序。收据与完整原始路径见 `evidence/impl/073/author-receipt.json`。

## 非作者审核退回：无 Package 的附加遍历

072 审核指出：初版 `Slots::new` 无条件分配字段外层表并再次遍历所有 local，即使函数没有 Package，也新增预算费用。这与本片避免无 Package 额外全扫描的冻结要求不符。旧测试 Pass 保留原状，该通过不覆盖此项成本要求。

修复在既有 managed-local 槽遍历中收集实际 Package 声明；没有 Package 时字段表保持空 Vec，不新增 local 遍历、表分配或字段费用。存在 Package 时才分配字段外层表，并对声明收集、表分配、真实字段扫描和责任状态分别收费。加入两个无 Package 的纯 Copy 控制（1 和 32 个 local/value）固定原资源阶段预算的 N/N−1 边界。

另一个费用变化单独保留并明确分类：原 normal-return 责任扫描以前没有为该遍历单独收费；初版已补上 `capture_start` 费用。它是旧工作漏收费的补足，确实影响含普通 Managed/拥有闭包但无 Package 的函数预算，不是新字段表成本，也不据此声称这些函数预算完全不变。加入没有 Package 的 normal-return/Trap 成对探针，证明两项普通责任的扫描差额为 2，并检查总预算 N 成功、N−1 返回 ResourceLimit；不改任何产品门槛。
