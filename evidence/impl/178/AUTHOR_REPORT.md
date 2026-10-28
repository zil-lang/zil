# 178：有限泛型 Store 协议（独审返修后作者限定验收）

## 精确身份与结果

最终返修源码为 `24402e17e1e248fb88a9a69fc5ec961388005e72`，采用 ZIL-LANG-1.4 / schema 22。此前 `fc2cdb9aef61a543faf94e61aa10750c09180996` 被独立审核 `4131416` 的真实定义期 create 来源反例退回；原作者结果、输入和失败完整保存在 `REJECTED-fc2-*` 及 runs 中。当前精确提交待非作者复审，旧作者通过不能代替独审。

返修后的干净源码运行五组 driver 回归 **62 通过、0 失败、1 筛除**，本片 **15 组通过**；五组 Expand 回归 **14 通过**。筛除的唯一 177 原断言要求已实现的泛型 store_new Unsupported，原输入实际为 Success / Verify / verification_runs=1，由集成者迁移，未计入通过。fmt、zil-expand/zil-driver all-target Clippy `-D warnings`、CLI 构建以及消费者均在本提交重新运行且通过；具体命令、环境和日志见 `SUMMARY.json`。

## 独审返修：新位置不等于新资源

原实现验证了拥有载体，却没有验证 `result(create)` 的新来源，导致未实例化的模板可以返回 give 入参。现在定义期另行跟踪新鲜性：store_new 与 result(create) 调用产生新来源，直接局部 give 保留它；give 入参、result(give) 调用、Package 字段取出不提供新来源。普通 Desc 别名不能因此升级。赋值读取实际 RHS，新鲜性在所有存活分支汇合时取交集，循环也必须保持这项外部不变量。位置替换的符号 ID 与此事实分开。未改 Types、Analyze、Verify 或默认额度。

新增原攻击及 **11 个来源/分支/字段/别名负例**，均在 Expand 拒绝且 verification_runs=0；**7 种真实新来源正例**分别在未实例化和实际实例化状态下通过。双元素 O0/O2 程序验证 fresh 工厂、局部多次 give 和两个返回分支。raw/Wire 另把 result(give) 改为 create，均被独立核以缺少 fresh 来源拒绝；本片现有 **4 类攻击 × 2 入口**，不是仅靠指纹不同拒绝。

一个更一般的正例仍暴露既有实现边界：`var fresh:Store<T>=store_new(); if flag { fresh=empty(); } return give fresh` 的实际实例会被 Types 降为内部 `$managed155` Package 字段，之后 Analyze 按字段 INPUT 边界拒绝 create。revision02 保留源、TypedIR 和诊断；没有把此路径算成通过。直接 fresh 局部 give、条件表达式选择 fresh、分支提前返回与 create helper 转发均实际通过。该内部替换来源保持需后续 Types/独立核协同处理。

## 实际实现

`StoreElement` 是规范保留的有限形状约束，不是可由空 trait/用户 impl 宣称的权限。未实例化模板也检查元素资格、名义身份、普通 Store 描述模式、实际 give/create 载体与失效别名。无约束 T 不会被当作拥有值；具体元素仍须是符合当前有限布局的非零完整 Package。资格不授予 Copy、Managed 或私有字段可见性。

同一普通泛型库实际支持 empty、reserve、push、pop、len、give 转发及 create 结果，错误容量返回完整未插入元素，错误联合与 `?` 检查真实两侧。完整 Store 字段的取走/补入保持实例来源；旧描述别名不会因补入新实例复活。泛型 Package 只在明确约束和拥有位置成立时接纳 Store 字段。保持外部消费状态和载体来源不变的有限 while 可反复弹出前缀；重复移动外部 T 或替换外部载体的循环仍拒绝。

生产改动集中在 Expand 的泛型语义层，未复用 Types/Analyze/Verify 的安全判定。实例化后仍走全部既有阶段及独立 Verify。真实名义 trait 方法和 Copy 静态外部值仍可按原协议组合，未知 bound 不会默认为 Copy。

## 实际消费者、失败与机器证据

四个真实源码模块把同一泛型 collection 库分别用于 Item（Ticket+i32）和 Other（Ticket+ByteBuffer+i64），并组合一个拥有具体 Store<Item> 与自包含 FnMut 回调的 State。O0/O2 共 **40 次原生执行**：10 次普通程序、30 次分配探针，其中 **22 次逐分配点失败注入**。所有受测分配数等于释放数且最终 live=0；每个 Build 只调用一次独立 Verify。输入 0–3 的退出码、回调计数、Drop 顺序、错误提前返回及无效输入均由执行前 oracle 规定。另有本片测试中的 12 次 O0/O2 原生执行，二者不混计。

四类 raw/Wire 攻击涵盖 Store 元素名义身份、Desc 冒充拥有元素、缺失前缀清理和伪 create 来源；每类两条入口均拒绝，并重算指纹避免只测试哈希不符。未实例化元素形状的精确工作边界为 2869 步通过、2868 步 ResourceLimit，默认额度未改。原 fc2 和最终返修 O0 的 Core / 接口 / Typed / candidate / Verified / LLVM 片段分别保留为阶段和机器表示证据。

开发期间的 Rust 编译错误、夹具 `>=>` 词法错误、消费者缺 bytes_reserve 的真实失败、旧断言和继承的 handoff Fail 都保留。旧 shell 包装器的 cargo 返回码未被可靠转交，因此仅按原日志判定，未填造返回码；最终命令收据记录真实退出码和干净源码身份。

## 边界和 Rust 四目标

本片是可复用 Store 泛型语言增量，不是完整泛型、全语言或 ZUI 产品完成。一般混合 T/F、一般 where/合同、嵌套拥有元素与外部责任循环转移仍未实现；store_at_mut 的语义属于 180，裸名识别不算能力。完整语言 NotSatisfied、产品 NotRun、B0 Doing；未运行正式全量，未改预算、产品阈值或性能门。

安全方面有源码/独立核验攻击与失败清理实证，仍限于这些有限协议。运行和内存方面证明受测行为与分配平衡，没有同语义 Rust 时间/RSS 比较。表达方面，同一源码库已实际服务两个名义元素与 callback State，但没有维护工作量比较。编译方面本片未做比较性性能采样，编译速度专项独立。因此没有四目标合取或产品级净突破结论；可复用泛型 Store 是明确的工程增量。
