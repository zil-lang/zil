# 195 非作者独立审查

裁决 **Pass**。审查者 `/root/resume188_acceptance`，实现作者 `/root/resume190`。精确候选为 `1a890dabc4eb04b26570439718b0a9432511fa95`；新增独立测试冻结于 `597f4a730d417c7652487f4c4fa3d6aa57bf025a`，两者仅相差一个测试文件，生产代码逐字相同。没有修改生产、schema、默认预算或作者输入。

## 实际执行与计数

| 本审查实际运行 | 精确源码 | 结果 |
|---|---|---|
| 作者 195 source / symbolic / raw 三组 | 1a890d | 31 / 0 |
| 190 Flow / range、普通泛型、178 Store、186 owned-success 邻接六组 | 1a890d | 73 / 0 |
| 独立 review195_by193 六项 | 597f4a | 6 / 0 |
| cargo fmt --all -- --check | 597f4a | Pass |
| 限定新测试的 Clippy，-D warnings | 597f4a | Pass |

最终计数 **110 / 0**，不累计 pilot 重跑，也不借用作者的 consumer 执行数字。上述计入的命令均由审查者在独立 `targets178/review195` 实际执行，启动时 checkout clean。命令 JSON 记录源码、参数、环境和原始 stdout/stderr SHA；复制后的日志保持原字节，已经逐项核对。

原作者 source 组在本审查重新执行了 82 次 Linux native；新独立输入在 O0/O2 各跑 0、1、2、4，共 8 次，因此本次 195 的源面 native 记录合计 90 次。邻接 suite 内其它 native 不加入此数。四模块消费者和 Wine 此次未由本审查执行，不能把作者的 72 次 / 50 次分配故障转写成本审查实证。

## 源码审查

检查了候选相对 `40abb53527aa45c89f6249162c9a69fea5fd80ad` 的完整生产差异；生产只涉及 `generic_check.rs`、`generic_check_enum.rs` 和新的 `generic_loop_control.rs`。

- `loop_control` 只向最近的 LoopFrame 保存状态；每条 continue 都单独运行原来的回边不变式。保存的状态按外层 local ID 比较，因此循环体的同名变量不能掩盖已经移动的外层值。
- break 不冒充 Return。循环后的合流包括零轮入口、真实 fallthrough、每条已检查 continue，以及每条 break；may-moved、Store origins 和 invalidated facts 被保留，fresh facts 要在所有存活路径上成立。
- If 和 match 只把能继续执行的分支送入普通合流；已返回分支不污染后续值，break/continue 的状态另存在所属循环帧中。while 条件本身的回边义务和 For 端点的源码求值顺序仍走原检查。
- `clone_loop_state` 在复制前支付可见名称、moved / store-owner ID 集、origin key，再由原 `clone_state` 支付类型、origin 内容、invalidated / fresh / owning-result 集。被遮蔽而仍留在集合中的 ID 没有按“看不见名字”省略。循环帧、出口快照和最终引用列表增长前均先扣既有预算。
- `impl190_for_ranges` 只迁移两个原 break/continue 模板的期待；格式字符串中的原 ZIL 输入没有改变。其它邻接断言保留。

Verify/Analyze/Types/LLVM 未被此实现改动。独立攻击从真实源码产生合法 TypedIR 后直接修改原始输入，更新 fingerprint 仅用于身份绑定；没有调用 Types 或 Analyze 为被篡改模块提供证明，raw/Wire 都由 Verify 自行拒绝。

## 新独立输入

1. 嵌套 for/while 和整数 match 同时包含 Return、break、continue。Return 分支交出 T 不污染存活路径；break 或 continue 分支交出 T 的近邻分别被定义时 may-moved / 回边检查拒绝，即使唯一具体调用传的是 i32。
2. Store 替换经内层 break、外层 break 传播真实 invalidated origins。无关 Store 的 alias 可以观察；原 current 的 alias 必须拒绝。替换后直接 Return 的路径不污染只 break 的观察路径。
3. 外层 for、内层 while、tuple match、FnMut 计数器和三层资源清理组合。独立手工推导输出为：输入 0 → `R`、退出 0；输入 1 → `ab1OR`、退出 10；输入 2/4 → `ab1Oa2bPR`、退出 24。两个优化级的实际 stdout、stderr、退出码与超时状态分别保存。
4. 在真实 break/continue 出口各重复一次 DropManaged，原 Flow 不改；raw/Wire 都精确命中 `E-VERIFY-RESOURCE` 的 duplicate automatic cleanup。这里改变的是真实责任操作，没有把 Break/Continue 标签互换当成必然错误。
5. 循环 callback 的 captured Desc 原本指向另一个资源；将实际 DescribeLocal 改指向仍有共享观察的资源后，raw/Wire 都命中 `E-BORROW-CONFLICT`。continue/break 控制边不隐藏这项实际外部效果。
6. 四层循环、长名称、已遮蔽 moved ID 和已遮蔽 Store carrier 的预算控制。nodes **1812 / 1811**、work **6042 / 6041**、depth **13 / 12** 分别在 N 通过、N−1 明确 Expand ResourceLimit，之后 fresh default 再通过。depth 是此完整源码的 Expand 结构门，不声称专属于四层 LoopFrame。

## 保留的真实失败

pilot01 为 5 / 1：审查者新预算夹具误要求 `Store<i32>` 通过，实际在 Expand 被现有 nominal-element 规则拒绝。pilot02 仍为 5 / 1：直接 resource Item 也不符合该 Store 元素 category。两条完整原输入在最终测试中逐字保留为负例，输入 SHA 及相等检查见 INSPECTION.json。真正的预算正控制显式声明 `resource R` 和 `package Item { manages r:R }`，没有解除旧门、提高预算或把前置拒绝算作预算成功。pilot03 已通过，但同样不计入最终 110。

## 证据范围与结论限制

MANIFEST.json 收录 2,046 份原始命令、源码、机器诊断、Wire 输入、native stdout/stderr 和预算记录，共约 3.65 MB。没有复制可执行文件、派生 LLVM 文件或大型可重建阶段 dump；完整原运行目录仍位于 `run-logs/195-review`。raw 基线 TypedIR 文本与 Wire 反例在紧凑证据中保留。

这完成了本切片的独立审查，不替代 root 的集成全仓验收。未执行 native Windows/Wine，也没有测量与 Rust 的运行时间、内存、维护成本或编译速度比较。四目标判断为：有限安全/清理行为有新实证；运行与内存成本未比较；普通控制与 helper 的表达组合增加，但维护优势未测；预算有真实边界而没有提速主张。产品级可重复净突破尚未建立，完整语言 NotSatisfied、产品门 NotRun、B0 Doing 保持。
