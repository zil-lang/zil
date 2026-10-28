# 202：完整 StoreElement 类型的 Types 实例化与作者收据

## 结果与身份

普通 `T: StoreElement` helper 现在可接收包含具体回调的完整元素类型，复用 `empty/reserve/push/pop/len/capacity`，并把 `pop<T>` 的成功值交给既有 callback helper。此前 201 的两份原始失败输入保持字节不变，最终均经完整 Verify 和 O0/O2 原生执行，输出 `1A2BZ`。五模块消费者也保持原源码；本作者测试覆盖 0、1、6 三个输入，完整故障矩阵由消费者作者和独审另列收据。

最终生产提交为 `ba4e4d07a1f14573871844f790a4daabc5efe9db`，主要前序为 `37542381`（完整 T 实例）和 `0098acdc`（未用定义检查与推断）。本报告只增加说明和小摘要，不改变生产。公开 schema/Wire 为 26，最终 design SHA256 为 `bb8821ef1c88f2753d12dd668fef0638da6dc849f541c55767051389f6387aa3`，normative SHA256 为 `10bd65c22970a3479f436ceab8baf5f58aa014ff94f780ec0896e160fd5c4bb5`。早期运行保留其实际规范和源码身份。

`final02` 与 `types03` 均在上述提交的干净工作树运行：driver 三个专项合计 **14/0**（source 8、raw 4、budget 2），Types 库 **18/0**；前者包含 **22 次实际 O0/O2 原生执行**。这些是作者限定验证，正式全仓、独审、CLI 身份与两平台故障矩阵不并入本计数，也不能以测试数量代替完整语言验收。

## 实现与约束

`store_templates.rs` 将普通类型模板与 callable 模板分开。实例键包含定义的模块、节点和完整 `Type`，不以 ClosureId 或元素名代替实际名义类型；类型实参在调用者作用域解释，再同时替换模板签名和正文。类型比较、复制、深度及实例增长先计费。实例只有成功完成才获得可调用的 FunctionId，有限混合计划检测活动依赖及未完成实例；实参仍只求值一次，经已有暂存与转交路径下放。

`helper_plan.rs` 同时调度普通实例和已有 callback helper。`resource_effects.rs` 区分具体函数与普通模板目标，真实参数位置参与 Desc 写投影，不使用占位函数号或默认只读假设。原无 TypeApply 的路径保留既有分类和收费。

所有普通模板即使未调用，也接受符号定义检查。每个函数的 T 仅在自身作用域有效；`StoreElement` 不授予字段、数值运算、Copy 或可调用能力。TypeApply 必须指向普通模板，且类型实参、位置及参数关系合法；单模块与 program 两入口都核验。对应 raw 测试由原源派生，有 14 个 Core 形状、28 次入口检查，其中 4 次合法控制、24 次拒绝。这是 Core 合同检查，不把未发射的定义体称作机器安全攻击。

每个实际 T 均加入已有 canonical `OwnedStore<T>` 资源定义，包括完全没有 Store 操作的 identity。该定义只承载未可信资格约束，由 Verify 独立计算实际布局和捕获资格；它不创建拥有值、运行时分配或清理责任。identity 的最终 Typed 含唯一对应定义且没有 OwnedStoreNew；零 stride 对照在 Verify 返回 Unsupported，非零近邻实际执行。不能把此拒绝重写为 SafeReject，也不声称未使用定义的机器代码大小不变。

最后的消费者修复在私有计划中区分错误和成功两侧形状：Name、显式 let 注解和 ascription 保留整个 union，`?` 只取成功侧。因此 `void!T` 可恢复弹出的 callback State，`T!void` 不会把错误载荷误当成功环境。显式 `void!Item<i32,F>` 由当前函数的声明 F 绑定求形状，不增加全局类型名优先权。旧 primitive 特判与预算不变；拥有 union 的模式匹配仍按已有关闭边界处理。私有 shape 单测不授予运行时权限，真实标签与责任继续由既有 Types/Analyze/Verify 检查。

## 原字节与精确预算

| 原始输入 | SHA256 |
|---|---|
| `impl202_fixtures/01_original_push.zil` | `101580f8d78e661011d3b73f731c4f2f2ae376a408c1cc02c8aa072d4f627bf9` |
| `impl202_fixtures/02_original_factory.zil` | `d8fd10675872542e99533ee23957db2c3150760a2f71ea8ff28150ad6c3a7a55` |

每行均实测默认结果等于精确 N、重新运行 N；N−1 为 Types ResourceLimit，三种额度分别核验。

| 输入 | Types work | locals | functions | 历史比较 |
|---|---:|---:|---:|---|
| 原 push helper | 20085 | 43 | 8 | 新路径本次测量 |
| 原 factory/reserve helper | 18959 | 41 | 9 | 新路径本次测量 |
| 201 原 02 | 4241 | 16 | 5 | work/locals 与已接受 201 同值 |
| 201 两个 Store 元素 | 11851 | 39 | 7 | work/locals 与已接受 201 同值 |
| 201 元素字段转调 callback helper | 11792 | 40 | 8 | work/locals 与已接受 201 同值 |

旧三个来源逐字复用。这里只对已有历史记录的 work/locals 声称不变，functions 是本轮实际边界；不把其它代理的 Expand 或 Analyze 数字当成此表的测量。预算界不是编译速度证据。

## 保留的失败及修复链

| 运行 | 真实结果 | 后续处理 |
|---|---|---|
| `check01`、`types01` | Rust 非穷尽匹配，编译退出 101，测试未执行 | 补公共新增节点遍历及旧测试构造后复验 |
| `source02` | 4/1，零 stride 已在 Verify 拒绝，测试误期待 SafeReject | 修正为实际 Unsupported；不改攻击输入或接受规则 |
| `source04` | 5/1，caller 内独立工厂与 Store 窗口组合在 Types 早拒 | 在已声明 F 的有限借入 Package 形状中补推断；最终源/native 通过 |
| `pop01` | 0/1，Expand 把调用者整个 error union 强绑给子调用 payload | Expand 作者的 `f200d5be` 修正结果上下文，保留 payload/错误侧近邻拒绝 |
| `pop02` | 0/1；原直接表达式已成功执行两次，显式 union 注解仍 Types 早拒 | 补当前 F 作用域的注解求形状，原源及显式形式各 O0/O2 成功 |
| 外部五模块 CLI02 | Types 丢失普通 pop 结果的成功 Package 形状，未进入原生阶段 | `ba4e4d07` 区分两侧；本作者 final02 的原消费者完成六次原生执行 |

早期 `source03/05`、`raw01`、`types02`、`consumer01`、`pop03/04`、`shape01` 的成功均绑定各自真实 HEAD 和 dirty diff SHA，不倒标为最终提交。尤其 source05 不标为 0098；consumer01 的六次原生成功也不替代 clean final02。部分 dirty 编译器补丁只记录了 SHA，未保存完整快照；这些运行保留原始输入、输出和命令，完整可重建的最终结论以干净提交为准。完整运行清单见小摘要及外置包。

最终错误侧互换的显式注解由 Types SafeReject，Verify 运行数为 0；它证明类型兼容门而非安全核攻击。无关早拒、未执行、私有模型与实际原生结果均分别记录。

## 交付与剩余边界

Git 只增加本说明和 `evidence/impl/202/frontend/SUMMARY.json`。命令、输入、Core/Typed/LLVM 文本、诊断与原生 stdout/状态单独压缩；可执行程序不入包，包内逐项 SHA256 和 ZIP CRC 完整校验。无作者独立 CLI 构建，故本收据不挪用 root CLI 的二进制身份。

本片仍限单普通 StoreElement 绑定，未新增 State 的 F/G 独立声明表、任意 trait impl、一般借入逃逸或新的 Store 参数角色。下一片只读规划，202 规范与生产保持冻结。

Rust 四目标分别判断：有限源码、Core 门与真实清理组合有实证，完整语言安全未证明；未做同语义 Rust 运行时间或峰值内存比较；普通库复用提高了实际表达能力，完整维护成本优势未测；精确预算保持不等于编译成本降低，本轮不做编译速度比较。产品级四目标合取、可重复净突破及完整 1.4 终态均不由本片宣称，原产品阈值不变。
