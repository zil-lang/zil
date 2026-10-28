# ZIL 1.4 / Ante-R1：205 State 与独立替换回调

本片把一个完整拥有的 `State<T,F>` 与一个独立拥有的 `replacement:F` 接入普通静态函数，贯通真实字段替换、FnOnce 消费后补入、跨函数转交、普通完整 T helper、动态 Store 以及同步失败清理。

正式完整工作区绑定干净 `2db3c090088dc3882a7566efe60f6cf6d81cc666`：**2552 通过、0 失败、0 忽略，259 组**；同一提交的全 workspace / all-targets Clippy 通过。冻结 CLI04 来自 `54452f8a50eb494c7c444bca3a9682802246284f`，其178份编译器输入与正式受测提交逐字一致。最终独立审核、集成身份和归档身份以 `evidence/integration/205/final/RESULT.json`、非作者 `REVIEW.json` 和 `ARCHIVE.json` 为准。

schema/Wire 保持27，当前规范摘要为 `9813f43ee7c86d786f197f0ed4b8e65938ee354d94b2cdaec858e09e7871ce5e`。完整1.4仍为 `NotSatisfied`，B0为 `Doing`，产品门、完整draft产品流水线及原生Windows均为 `NotRun`。

## 普通库现在可以表达什么

新的普通函数可以取得一个完整 State 和一个实际替换值，返回同一完整名义实例类型。State 可以有一个或两个同 F 的直接 callback 字段；两个拥有参数可以反序，也可以被普通 Copy 参数隔开。普通/phantom T、模块和名义身份参与完整约束，不能因两个工厂的环境布局相同就混合类型。

F 是类型绑定；State 的两个字段和 replacement 是至多三个实际运行位置。它们各自保留初始化状态、调用能力、借用窗口、移动和清理责任。FnOnce 可以分别消费各位置一次；用 replacement 补回缺失字段不使旧别名或已消费的 replacement 复活。普通函数可以构造另一份同类型完整结果，不要求返回原物理对象。

替换仍先完整准备 RHS，再清理旧字段，最后发布新字段。后续实参准备失败时，已转交到调用临时的 State 按普通退出规则清理；并不回滚先前显式转交。旧字段、已准备临时及未使用 replacement 的责任分别结算。

五模块消费者实际组合了204的叶 Store 拥有捕获：三个同 F 环境各自拥有叶 Store、Ticket 和可变计数器，经完整 T helper 进入外层 Store，在实际元素位置调用，pop后替换、消费或退回完整 State。零次/一次/continue/break、反序参数、开始前/开始后的准备失败均有固定机器oracle。这里的退回或提前返回是同步控制流，没有新增异步取消权限。

## 施工中发现并关闭的缺陷

### 未调用的旧 State 定义漏检

含普通 T 的旧单 State 坏正文可能被展开器省略；去掉 T 后保留 Core 也不代表两个公开 Types 入口实际检查了旧借址失效。原输入曾报告 Success / Verify1，只核了 main，非法程序没有原生执行。

本片在 Expand 定义检查及两个公开 Types 入口分别补足真实来源、位置和别名代次检查。raw Core 反例由真实合法程序产生，旧 alias 跨 take/refill 后使用必须独立拒绝，不借用前端给出的“安全”结论。

### 合法循环刷新被误拒

最初的别名合流把已不再使用的表达式快照当作下一轮局部变量的来源。另一个 continue 近邻又暴露了循环体局部已经移出、但跳转快照仍保留其活性的问题。

修复按实际借址 key、借址版本和分支映射判断回边，只退休本轮创建的局部；外层移动和失效代次保留。原698字节合法实例及相同正文的0/1/3次实际循环均通过；旧副本、未刷新回边和仍活的失效别名继续拒绝。原失败与修复后执行不混写。

### 普通完整 T 推导漏掉单 F State producer

真实消费者的 `pack → identity` 最初在 Expand 被拒绝。既有 producer 信息只覆盖双 F/G，漏掉了203已承诺的单 F 完整值推导。修复补充词法范围内的完整名义结果信息，仅用于延迟实例化，不授予 Store 资格或 caller 的 F 权限。两个同布局不同名义 T、独立工厂和 phantom T 仍受完整约束。

### 旧合法完整 State 右值转交回归

首轮整仓发现原867字节程序 `(next)=relay(next)` 被 Types `E-STATE-ORIGIN` 误拒。旧单 State 头把 callable bound 放在完整 Package 参数上，新检查误把整个 Package 当作 F 来源嵌进结果字段。

现在仅从该参数的直接 `Named(F)` 字段提取并统一真实来源，再按原声明能力代入结果。原程序、Success预期与O0/O2输出保持；F→G重命名、双字段、声明FnOnce、不得加强能力及转交后旧alias不复活均有近邻检查。独审还实际执行了多层完整转交和三个独立可变环境的组合。

## 实际验证与归因

| 执行者与范围 | 实际结果 | 身份与限制 |
|---|---|---|
| root正式完整workspace02 | 2552 / 0 / 0，259组 | clean2db3c090，193.7716秒；不是编译性能比较 |
| root全workspace/all-targets Clippy | Pass | 同一2db3c090，`-D warnings` |
| 作者CLI04 Linux五模块 | 140 / 0，88次真实分配失败 | CLI04与既定源码/oracle，全部最终live0 |
| 作者CLI04 Win64/Wine五模块 | 140 / 0，88次真实分配失败 | 全部最终live0、bad_operations0；不是原生Windows |
| 非作者CLI04 Linux五模块 | 140 / 0，88次真实分配失败 | 独立运行；不与作者或整仓计数相加 |
| 作者最终Types窄测 | source16、raw7、budget2均通过 | 58次native；其它原177/producer有限回归各自留证 |
| 作者四处旧阶段迁移 | 三个旧测试binary共31 / 0 | 52次原生正例，四处非法原源不变 |
| 非作者最终返修近邻 | 原177、raw转交、三预算及6个精确阶段检查通过 | 真实执行身份、早期source/raw/Wire集合与静态审查分列 |

不同范围存在重叠，不能将表中数字累加为“总覆盖”。较早CLI03的真实通过仍属于CLI03；其后的实际生产修复由CLI04重新验证。

环境容量有限，workspace02仅通过标准 `CARGO_PROFILE_TEST_STRIP=symbols` 剥离宿主Rust测试可执行文件的符号元数据；测试仍为未优化、debug0，原语言输入、oracle、逻辑额度及编译器生产源码未变。该环境差异完整写入命令计划，不把宿主测试产物声称为与首轮相同。

## 原始失败和预算迁移

首轮正式workspace01在clean0317a093实际为 **2544 / 6 / 0**。一项是上述旧合法程序回归；四处为同一非法原源现在更早由Expand或Types拒绝；一项为旧Types工作预算硬断言。原始输入、阶段产物和命令保留，首轮仍记Fail。

阶段迁移只涉及197的两处、201同组第三个独占窗口反例及184的whole-parent move。201前两个callee/parent反例仍由Verify拒绝，原生正例和全部源码字节保持。Clippy首轮还发现迁移后唯一未用的测试辅助函数；删除该函数后同源第二轮通过，没有放宽警告。

旧定义安全检查有真实新增工作，按已登记规范条款保留旧N、原断言实际Fail，再逐输入测N、N−1和fresh N。三个旧callback Store输入的Types边界如下；其完整Typed结果在默认/N/fresh N下相等，N−1准确为ResourceLimit。

| 输入 | 原工作N | 新工作N | 局部N | 函数N |
|---|---:|---:|---:|---:|
| old-original02 | 4241 | 6326 | 16 | 5 |
| old-two-elements | 11851 | 29030 | 39 | 7 |
| old-nested-helper | 11792 | 29566 | 40 | 8 |

这三份测试与源在已接受204整仓中仍通过原N；逐字身份及原命令已核对。205另有Expand1731→1938、Types2642→5293的具名旧别名检查迁移；未触发额外工作的一般Store和普通eager控制保持原值。新头、producer、循环、CT/raw/Wire费用各有独立源和边界记录，不将一个输入的N泛化给整个profile。默认额度和产品阈值未提高。

## 交付与下一阶段

交付标准完整main Git bundle及204→205增量bundle，保存可审计历史；旁置JSON固定提交、tree、前置对象、大小和SHA256。离线导入只建立不可变语言incoming ref，用户在独立集成分支合入实际编译支线。现有一键脚本未提供，故只验证标准bundle与随包import helper，不冒称验证过该脚本。

新源码Git只收录代码、测试、工具、文档与紧凑收据。完整源码/raw/Wire/IR、二进制标准I/O、命令与原失败独立无损去重压缩；执行产物仅保哈希，Cargo缓存不交付。最终容器大小、SHA、恢复及独审见ARCHIVE.json与离线说明。

下一206优先有限完整嵌套Package Product：父字段持一笔完整child责任，取出为独立局部后可部分操作；逐Managed叶保留Created/Incoming，不把装包或取子包当作新建资源。允许只含child的自然wrapper，不要求多造dummy资源。普通完整值准入与旧capture/Store/Sum/owning-array资格分开，深层可操作Place不随之开放。

更一般的拥有组合、traits/关联项/life/HRTB/lending/dyn、卫生Syntax与Type CT、长期外部回调、挂起/异常/取消/并发以及产品量化条件继续保留。205解决一个明确的ZUI语言前置，未声称所有前置或完整1.4已经完成。

## Rust四目标与突破判断

| 目标 | 当前实证 | 当前判断 |
|---|---|---|
| Rust同类安全 | 实际多位置、未用定义、别名代次、source-derived raw/Wire、失败清理及发现后修复的回归 | 本片有限行为通过，完整语言安全未证明 |
| 运行与内存同级 | O0/O2真实行为、两平台分配/释放、峰值4份活跃allocation及264B请求payload | 请求payload不是RSS；未做同语义Rust时间/RSS/尾延迟比较 |
| 表达与维护更简单 | 普通库可接State与替换值，组合Store、完整T与跨函数更新 | 库级表达增加，完整框架维护成本优势未测 |
| 编译成本更低 | 所有新增实际检查计费，具名旧预算变化透明保留 | 本片不提供编译速度结论，速度专项独立 |
| 至少一项实质突破 | 真实状态替换组合和安全检查缺陷闭合 | 属工程进展，产品四目标合取与可重复净突破仍未建立 |
