# ZIL 语言阶段 161–165：嵌套拥有回调、名义约束与资源方法

日期：2026-10-27。阶段结论：**StagePass_FullZuiNotReady**。本轮从159恢复包接续，三条语言实现均通过非作者审核并合入；组合消费和当前整仓验证通过，原六项缺口探针关闭两项。完整语言仍为 NotSatisfied、正式 B0 为 Doing、产品门 NotRun。

## 源身份与恢复基础

恢复包 `zil-progress-159-20261027.zip` 的 SHA256 为 `189890c75518ad61c66f6438053032299abbdc1b288529256547dc7a32d84919`，恢复基线为 `e9a604c80d56027a5fb6c28bcb8b58f9c9e84a7b`。使用自包含 bundle 重建了独立主树、实现 worktree、审核 worktree 和分开的构建目录，未同步 GitHub。

本轮实际工具链是 Rust 1.99.1、Clang/LLVM/LLD 17.0.6、Linux x86_64。159记录的 Clang 为17.0.0，补丁版本差异已保留；没有继承旧测试次数。本次首先实跑恢复基线，1328通过、0失败、0忽略。规范、Cargo.lock、产品门和默认预算均未修改。

语言三线合入提交为 `712a9602797891083fe3f549381e1e12ab34dd02`。165只迁移旧回归测试，最终统一受测提交为 `03aa25206fbf4c5d783c5fe0ecc4078fbe628b73`，`crates` tree 为 `b3c6ea961e3e3b29021a83b7880f4e2c54407cb0`。之后交付文件与审核记录的提交可以不同，交付身份以包内 `RESTORE165.json` 为准；语言生产源码与712a960逐字一致。

## 已实现的语言能力

| 任务 | 实际增量 | 仍保留的边界 |
|---|---|---|
| 161 嵌套闭包 | 多级词法捕获、嵌套 move 转交、Copy 快照与写回、真实拥有捕获在内层环境的唯一清理；Package 转交接入槽状态分析 | 没有开放一般闭包值再捕获、闭包返回闭包或回调集合；当前有限环境形状仍受原规则约束 |
| 162 名义 trait 约束 | `<T: Read>`、多个静态约束、未使用泛型正文的符号检查、泛型转发、共享/独占/按值方法权限及跨模块名义身份 | 当前是普通结构体的有限静态非泛型 trait；未开放 where、supertrait、关联类型、dyn、新外部 trait impl 或一般泛型 trait |
| 163 资源方法 | 默认 Desc `self`、显式 `give self`、公开 Resource/Package 跨模块方法、本模块 ByteBuffer 方法；方法效果传播到拥有闭包的 Fn/FnMut/FnOnce 判定 | 真实资源/Package头地址 `&Self` 仍未实现；ByteBuffer跨模块扩展方法查找和一般拥有聚合未开放 |
| 164 组合验证 | 四模块消费者贯通三条能力、独立错误准备路径、保持原六探针、记录/恢复脚本及实际恢复验证 | 是语言消费者，不是完整ZUI；未运行旧35项正式门或性能研究 |
| 165 回归迁移 | 将旧泛型约束“一律不支持”测试拆成准确拒绝与合法未用约束正例 | 只改测试、提案和证据；没有调整生产规则或新增语言权限 |

161保留了内层非move闭包对外层拥有Copy状态的真实写回。163的Package方法先消费整个接收者，如果所有返回路径都完整补回，可继续为FnMut；错误在补回之前返回则要求FnOnce。审核暴露的初版误分类先复现再修复。资源的函数指针字段继续按字段解释，不因 `r.call()` 的形状被误当作写方法。

162要求实际名义 impl 证据；方法名字相同不足以满足约束。未声明的 `Copy` 不是自动拥有特殊资格的内建约束；声明同名普通 trait 也不能制造权限。未知名称和同名 struct 均精确拒绝。

本轮复用了现有责任指令和机器通路，未改生产 Analyze、独立 Verify 或 LLVM 后端。每个新增原生正例仍须经过独立 Verify；raw/wire 对抗核验分别保留在161与163审核证据中。

## 原159六项探针的原样复跑

原探针脚本 SHA256：`1acf26ce2b3b74629a4936d74ab0f943ce71d6751f1843c2a3a64d7241520f45`。没有用较弱的新输入替换旧缺口。

| 原探针 | 新环境基线 | 本轮集成候选 | 判断 |
|---|---|---|---|
| nested-owning-callback | Unsupported | Success，Verify=1 | 本探针关闭 |
| generic-trait-bound | Unsupported | Success，Verify=1 | 本探针关闭 |
| owned-array-elements | SafeReject | SafeReject，Types | 未关闭 |
| owned-error-result | Unsupported | Unsupported，Types | 未关闭 |
| owning-method-receiver | Unsupported | Unsupported，Types | 未关闭，原输入要求真实资源头地址 |
| array-of-capturing-callbacks | Unsupported | Unsupported，Types | 未关闭 |

数组探针的 `R` 注解默认是Desc，而构造产生Managed，因此先报模式不匹配；该拒绝不能作为一般拥有元素析构的实现证据。原接收者探针要求 `&Self`，163的Desc/give方法不能替代它。这六项只是原有回归清单，也不代表所有语言前置要求的完整枚举。

## 实际验证及失败记录

| 范围 | 本轮实际结果 |
|---|---|
| 恢复159后重新执行完整workspace | 1328通过、0失败、0忽略 |
| 集成首轮workspace | 1381通过、1失败；原件保留 |
| 165合入后重新执行完整workspace | **1384通过、0失败、0忽略；143个结果组，进程退出0** |
| 完整workspace/all-targets Clippy `-D warnings` | 通过 |
| 完整workspace fmt | 通过 |
| 冷恢复候选的161/162/163专项 | 新目录、新target，53/53通过；绑定e158fdd及当时crates树 |
| 164记录/恢复工具独立控制 | 8项通过；合成元数据用例，未算入语言测试 |

首轮唯一失败在 `zil-expand/tests/generics.rs`：旧测试将未声明的 `T: Copy` 与已注册但未执行的泛型 `#[inline]` 都要求E-UNSUPPORTED。新名义约束实现对前者返回Expand/SafeReject/E-GENERIC-ARGUMENT。165经非作者审核后，仅迁移这条旧契约：未知约束准确拒绝，未使用但已声明的Read约束及符号方法正文真正通过，未执行inline属性仍Unsupported。真正未知的属性依旧在Parse阶段返回E-PARSE-ATTRIBUTE-UNKNOWN；没有为了测试通过改变生产诊断。

作者、非作者、统一回归、冷恢复各自记账，不能把重复运行相加成“更多独立样例”。完整命令、退出码、源码身份与日志SHA见 `evidence/impl/164/integration.json` 及各任务收据。所有初版失败、审核夹具纠正和工具错误接受控制均保留。

### 四模块真实消费者

`examples/mainline/zui_language164` 将 `model` 的公开名义trait、`adapter` 的约束泛型、`ticket` 的资源工厂/读取/消费方法和 `main` 的嵌套拥有闭包组合起来。每次执行1024个事件，逐一检查返回值，再核对ByteBuffer中1024个字节；Ticket的唯一析构点打印一个点，因此预期stdout恰好1024个点。

root在O0/O2分别运行普通原生程序与分配探针，共4次执行。独立审核者从另一worktree和target构建并重复上述4次；每个优化级的分配探针都观察到allocated=1、frees=1、live=0、peak_bytes=1024。它们是具体消费者的行为证据，不是相对Rust的运行或内存优势测量。

独立审核另写四模块错误消费者：在两层拥有闭包里准备 `(give payload).consume(fail()?)`，普通/探针各O0/O2共4次执行，全部按预期返回1，仅析构一次Ticket，ByteBuffer释放后live=0。此例检验接收者责任已经准备、后续参数失败时的清理；没有用abort替代同步错误清理。

### 审核身份

| 任务 | 实现交付 | 非作者审核 |
|---|---|---|
| 161 | `84af56e5185abc2fa1ac2e62f3395638fe0c3788` | /root/trait_bounds；14次独立native、5源码拒绝、6次raw/wire拒绝 |
| 162 | `ee2e8b04de7829491f8168eb54ccceedab72f57d` | /root；6次独立native、4源码拒绝 |
| 163 | `f5a5b532b3410474495a43780c63d4a0dc1fbcf3` | /root/nested_closures；单163与161+163组合共28次native、11源码拒绝、4次raw/wire拒绝 |
| 164 | `712a9602797891083fe3f549381e1e12ab34dd02` | /root/trait_bounds；正常/错误消费者8次native，工具控制8项 |
| 165 | `a821e945143e19b23a0fff8896bd80759f582652` | /root/nested_closures；generics4/4及独立源码近邻5/5 |

163的公开owner导出小补丁由root编写、/root/source_inventory审核，精确原提交及cherry-pick身份写在163收据；没有把该部分算成root自我审核通过。

## 可恢复交付与历史状态

包内包含完整源码ZIP、自包含Git bundle、所有现存refs清单、恢复脚本、当前收据和原始执行证据。候选包已独立恢复，核对20个引用、97个可达提交、6个工作树及2701个源码文件的内容和Git权限。ZIP普通文件的默认mode编码一度被审核器误判，已通过真实普通/可执行/符号链接控制纠正并保留初次失败。

最终包在添加165及报告之后另做引用和源码核对，最终结果以外层 `DELIVERY-VERIFY.json` 为准；候选包的53项冷测试没有改写为最终包的首次执行。恢复使用本机已装工具链与Cargo依赖缓存，构建target为空；工具链二进制和依赖缓存不打包，不能读成裸操作系统离线部署。

旧 `node tools/handoff.mjs --check` 中snapshot-only/Fail仍保留：恢复时已有规范登记摘要不一致、旧提交对象和历史审批证据缺失。新任务的实现提交继承该规范登记问题，因此也不能宣称正式handoff通过。本轮不伪造旧审批对象，不重标旧正式门成绩。当前原生结论属于Linux环境；原生Windows、完整语言与产品门均未由本阶段自动完成。

## Rust四目标与下一阶段

| 目标 | 本轮结论 |
|---|---|
| Rust同类安全 | 新有限语言组合通过真实清理、精确拒绝和独立Verify对抗检验；完整语言安全尚未证明 |
| 运行与内存成本 | 观察到具体程序正确释放与零存活；未做同合同Rust时间/RSS比较 |
| 表达与维护 | 普通库代码现在能组合嵌套拥有回调、名义约束与资源方法；维护成本优势未测 |
| 编译成本 | 按用户拆分安排，本轮不做编译提速研究，也不继承旧性能结论 |
| 实质突破 | 可使用的语言能力有实质增量；没有宣布产品级Rust净胜出 |

下一阶段按 [三线计划](NEXT-LANGUAGE-165.md) 推进：拥有错误载荷、拥有元素聚合/容器、具体闭包转发与保存先并行；真实头地址单独形成来源、搬迁窗口、独立Verify和ABI闭环。需要公共schema的新载体先固定具体合同，不能只放开前端接受或自动引入装箱。
