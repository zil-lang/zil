# 193：同步显式有限 Copy 切片参数

## 范围与实现

本片按主树193派工和1.4设计§15.2开放 callable 的显式 `&[T]` / `&mut [T]` 参数，以及同步非逃逸 helper 的附加切片参数。T 限原有有限 Copy 值；回调的 Address 参数仍要求 Copy 结果，新增 helper 切片参数也要求 Copy helper 结果。环境 Fn / FnMut / FnOnce 与显式参数的 shared / exclusive 资格分开。

实现基点为 `c860a100677c15b19ee1694aa66ddf81d8aee926`，规范摘要 `d66bf6d16f2c1adecc4fe8e03d4898560acc88e6406e798c1c13f424287f7f64`，normative摘要 `d20398d8a0291e1e8eef74bf74c557e992fa176e7e5abe09f924071fd1cc2a25`。生产修改最初冻结为 `03c20e5b05c8066ed703d6d00e3968e9ac134585`。最终实际测试身份、原始命令和保留文件散列见 `evidence/impl/193/author-result.json`。

Types 的签名分类、Graph Address 约束与投影识别借用切片的现有 pointee 结构；mutable slice 不再误用只处理 shared lending result 的旧旁路。helper template 只调整参数准入，不改190的 Flow 构造。Analyze 和 Verify 分别检查具体元素、参数结果限制、实际 entry 与合同；Verify 使用自己的 `slices::check_pointee`。Lower / LLVM 仅扩展参数准入，沿用现有 `{ ptr, i64 }` 机器表示和实际 operand 发射。

没有修改 schema24、Wire、Type 的 Copy 定义、默认预算、loan 算法、借出结果来源或环境布局。没有开放 slice capture、显式借参返回、跨调用存储、裸 Slice 值、嵌套 Address、非 Copy 元素、FFI 或 CT。

## 原输入与独立权限检查

原193预检19个源输入按原字节放入新 source suite：14个 Success / Verify，5个 SafeReject。其中 captured Desc 写入同 backing、显式 shared / exclusive 实参同 backing 都在 Verify 报 `E-BORROW-CONFLICT`；异源和 shared + readonly Desc 对照成功。共享写入则在 Types 的既有权限入口拒绝。本组仅承诺源面到相应阶段；其中忽略显式 buffer 准备失败的旧预检输入不冒充原生成功程序。

新的 raw/Wire suite 从真实源面生成 TypedIR，再保留或变造声明、执行 operand、Hold、reborrow 权限、范围端点、捕获与结果来源。既更新 module fingerprint 又保留故意过期的安全声明，避免把 stale identity 当作权限证据。变造 backing 后先保留 stale Hold，确认 `E-VERIFY-HOLD`；再提供按真实 operand 重建的未核 candidate，确认实际重叠仍为 `E-BORROW-CONFLICT`。同一 exclusive SSA 重复实参先被 `E-VERIFY-MOVE` 拒绝，另有两个不同 SSA carrier 指向同 parent 的 `E-BORROW-CONFLICT`。这些入口分别保留，不能互相代替。

初测 raw 和 Wire 的实际最小工作预算都为 **11617**，11616 返回 ResourceLimit；最终冻结重跑记录在预算命名的原始报告中。参数数量和 slice 元素树深度还分别覆盖有界拒绝。未提高默认额度。

## 机器消费者与失败清理

四模块 `examples/mainline/slice193` 包含：六个 fat-window 实参越过整数寄存器参数数量、Copy record / fixed-array 元素、Unit 与空范围、对象与两个端点按123顺序单次求值、顺序重借、Fn / FnMut / FnOnce、跨模块 factory / helper。显式 slice 参数不会进入工厂拥有环境。

拥有 callback 仅保存日志 ByteBuffer 与 Ticket。每次成功调用显式 reserve 一字节日志容量，随后修改传入的固定数组；失败路径在修改数组前返回。输入0、1、2、3的普通输出分别为 `AH`、`ABH`、`ABCH`、`ABCDH`，退出码0；非法输入与EOF无输出、退出码9。第k次分配失败保留此前成功输出及唯一Ticket清理H，退出码1；全部实际分配与释放数相等、live为0。最后一次分配以后的故障索引保留正常结果。

runner 将未修改的原程序与仅重定向allocator导入的单独探针区分。每个平台 O0 / O2 共40次执行：12普通、28instrumented、12真实分配失败。Linux称 native；Win64 / Wine 只称兼容执行，native Windows为NotRun。机器越界与反向范围另外以真实checked trap覆盖，不当作可恢复错误清理。

## 保留的失败与夹具迁移

- `consumer-draft01`：新草稿对临时closure取共享地址，命中“shared borrow requires a place”。给closure显式局部名称修正草稿，生产不变。
- `source01`：共享到独占的正确诊断实际是 `E-ADDRESS-PERMISSION`；`&&` 被词法当逻辑运算符，改用原嵌套类型的 `& &` 写法；另一个原生测试忽略 `bytes_push` 需要先 reserve，空view实际触发checked trap。保留原源码和失败，修正新测试的准备步骤及精确期待，不改原19输入。
- `raw01`：期望最晚loan诊断，但真实更早的 `E-VERIFY-HOLD` 和 `E-VERIFY-MOVE` 已拒绝。分开保留这些准确负例，并增加正确candidate / 不同SSA同parent后到达loan的反例。
- `regression01`、`verify01`：090 / 189 / Verify091旧夹具把显式slice参数设为关闭。经root在主树 `8ebdea24f2105e8c6f05be8197a024ef71f5345a` 授权，按原输入迁移：合法unused参数到Verify成功；仅改声明、entry仍为`&i32`的raw反例仍被 `E-VERIFY-CLOSURE-ENTRY` 拒绝；slice capture仍Unsupported，空lending来源结果仍为184规定的 `E-VERIFY-LENDING`。其余安全断言保留。
- `win64-01` 因sandbox阻止Wine本地socket未运行程序；获准后的 `win64-02` 发现工具链ntdll.dll为空。root确认旧prefix链接缓存清理误伤，并从原始deb按清理前SHA恢复全部726项，收据 `run-logs/185-integration/wine-runtime-restoration-08.json` 为Pass。`win64-03` 随后完整通过；这些是环境故障，没有为其修改193生产或PE。

## Rust四目标

| 目标 | 本片证据 | 界限 |
| --- | --- | --- |
| 同类安全 | 源面原输入、独立 raw/Wire、真实 backing / 当前调用资格、范围和预算负例 | 同步有限profile；不等于完整语言安全、一般逃逸或并发取消证明 |
| 运行与内存 | 原fat-pointer表示，实际执行、逐分配点失败及唯一清理 | 没有同语义Rust计时、峰值内存或尾延迟比较，不宣称同级或胜出 |
| 表达与维护 | 普通通用访问器可接收动态长度的共享或可写窗口，复用现有helper与工厂 | 库组合能力增加；没有维护成本量化结论 |
| 编译成本 | 原schema/ABI与预算体系不变，新增形状仍有实测N/N-1界限 | 未作编译性能采样，不把少量生产改动当提速证据 |
| 至少一项实质突破 | 文本/缓冲处理的一个具体前置闭环，可实际跨模块执行 | 属工程能力增量，未证明产品级净突破 |

完整语言 NotSatisfied、产品 NotRun、B0 Doing 保持。193的独立验收与主线整合归root，不由本作者提交替代。
