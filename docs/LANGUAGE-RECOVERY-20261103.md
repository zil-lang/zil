# 299基线接续：有限模式、泛型与Product借用

本轮从真实可用源码 `7f039d16dd65c3608dc3a909bc9628f3db9a975a` 新实现313–317、320–321。用户列出的301–312旧对象、未提交修改及集成树均未恢复；补充对话说明记录旧成果未推送，不是恢复附件，也不覆盖现行规范。恢复范围见[审计](RECOVERY-20261103.md)。

共同候选 `a57a668b68044e37e870d058684f9364307e04b1` 的本地检查通过。源码已集成到本地 `work`，远端备份分支为 `task/language-recovery-20261103`；远端main仍为恢复基线，本报告不表示main合入、远端CI通过或完整语言完成。精确提交、命令、输入与日志摘要见[机器记录](validation/recovery-integration-20261103.json)。

## 实际新增范围

| 切片 | 本轮结果 | 保留边界 |
| --- | --- | --- |
| 313、321 | 有限Copy tuple的let/var解构、嵌套/弃置、精确arity、作用域、RHS一次求值；泛型定义期符号消费者也已接通 | 未开放ref/拥有tuple模式或任意一般模式；未用泛型定义同样检查 |
| 314、320 | Type/Syntax与普通Copy泛型函数、具体纯记录组合；完整名义/phantom身份、别名作用域、累计预算；Struct直接Copy约束由运行期消费者兑现 | 不代表const/life泛型、泛型trait、关联投影或任意CT别名/enum已完成 |
| 315 | 从普通完整Product局部建立`&mut`，传同步helper修改顶层Copy字段；Analyze/Verify独立重建完整性/来源，Verify裁决冲突 | 不开放新的Store/Closure/Sum、深层拥有子对象更新、逃逸或异步借用 |
| 316 | 25份真实源核查：6合法控制、5安全拒绝、11待实现入口、3life标注探针 | 是能力盘点，未实现life/trait/并发；标注擦除问题不能当有效life契约 |
| 317 | 四模块组件将CT泛型Config、tuple、Product可变helper和已有scoped callback组合；成功、业务失败、空登记均核对顺序和清理 | 同步有限消费者，不代表ZUI框架或跨线程登记实现 |

TypedIR/Wire版本保持；公共Core新增TupleBinding已接所有相关遍历器。没有新增Cargo测试目标或隐式Box/RC/锁。普通类型、生命周期、trait、动态抽象及并发剩余项以[当前清单](LANGUAGE-REMAINING.md)为准。

## 本地检查与独立复核

| 检查 | 实际结果 |
| --- | --- |
| 默认完整工作区 | 4055通过、0失败，422组，0忽略/过滤 |
| reference完整工作区 | 3947通过、0失败，422组，0忽略/过滤 |
| LLVM text-reference | 64通过、0失败 |
| 迁移runner | 9通过 |
| fmt、all-targets Clippy | 通过，Clippy以`-D warnings`运行 |
| 架构依赖、LLVM工具链 | 15 crate/38边通过；LLVM/Clang23.1.4核查通过 |
| 317机器组合 | clang与llvm-api实际两后端×O0/O2×3输入，共12次，未fallback |

317三个预期输出/退出分别为`REVSPBCAH`/0、`REVFPBCAH`/1、`RNBAH`/2。最初未启用driver的llvm-api feature，只有6次clang执行；错误的12次表述已纠正，随后显式启用feature完成12次实际执行。作者、独审与整仓重放不相加成新增覆盖。

非作者318核对313/315/317/320/321、实际raw/Wire和223历史窗口；非作者319核对314的CT身份、作用域和预算。精确实现身份及有限裁决见[318](validation/recovery-318-review.json)、[319](validation/recovery-319-review.json)。review报告随后合入只改变元数据，不改受测代码和注册测试。

原失败没有抹去：初次整仓编译发现测试遍历器未消费Core新variant，补了真实初始化效果覆盖；第一次完整默认4051/3、reference3943/3，三项旧223测试仍要求一律拒绝可变Product参数。独审确认原输入只写完整Product的顶层Copy字段，符合315当前合同，之后迁移期望、保留原源与Wire字节，并增加精确共享写/拥有子对象/缺子对象反例。版本27/29/30的E-WIRE-VERSION继续拒绝。一次测试Clippy格式性失败也保留记录后修正。

## 未关闭的实际问题

原规模262144字节嵌套数组已在当前LLVM23.1.4重新复现O0崩溃：clang进入SelectionDAG后失败，API CLI直接SIGSEGV。API O2成功；clang O2在2GiB限制下OOM，另一个较小控制也资源受限未完成，均单独列为资源受限未完成。没有把缩小输入、O2通过或历史内存SSA提案当作修复。复现源及[精简报告](validation/recovery-large-array.json)已收入仓库，生产修复待下一切片。

恢复时handoff有1966项完整性错误；最终显式检查仍Fail（1976项），除历史对象/摘要债务外，旧工具要求主树分支名main及evidence/路径，本轮work候选的14条精简docs/validation引用因此被拒。现行规范注册摘要已按实际字节更新，没有为迁就旧工具重建原始证据目录。check-plan的28项旧规范hash债务仍保留，未将这些检查改成Pass。此次没有远端CI、原生Windows或同语义Rust性能比较结果。B0 Doing、完整1.4/ZUI NotSatisfied及产品条件保持。

## 源码备份与接续

每阶段批量提交并推送候选/任务/审核分支，源码历史bundle放在`/workspace/backups/zil-20261103/`，经`git bundle verify`核验。本地bundle配合已核对SHA的远端分支保存，原始临时日志、target缓存、附件私有环境标识不进入Git。最后推送的精确头与bundle校验值由同目录`final-manifest.json`记录，避免报告记录自己的提交号造成循环。

下一批从[主线优先级](LANGUAGE-NEXT-PRIORITIES.md)和现存失败源码继续；已经贯通的四份历史pending源保留原字节作为正控。若平台以后提供旧源码，再比对后集成，不重复按旧编号重建或宣称已恢复。Rust四目标的有限实证与未测范围见[Rust对照](RUST-GOALS.md)。
