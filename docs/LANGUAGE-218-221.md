# 218–221 公共拥有工厂与源级条件选择

2026-10-30。本批有限语言能力已通过独立审核和完整 CI，受验组合源码为 `69931f83c11f8127d0d4aebe4e2bd6fe2aaeffbf`。四个切片联合合入 main，正式父任务 B0、181、完整 1.4、全部 ZUI 前置和 Rust 产品目标仍未完成。

## 实际新增的能力

### 公共函数接收拥有回调并返回新的闭包

218 的 Expand/Types、220 的 Analyze 与 221 的 Verify 共同接通一个有限公共接口：普通自由函数按值接收一个直接 `F:Fn/FnMut/FnOnce` 参数，返回一个新的、自含的 opaque 闭包环境。该函数可以包装、提前调用、消费或丢弃输入 F；返回值不必保留 F，但不能把原 F 原样当作新的环境返回。

有限 Copy、直接 Managed 和完整可捕获 Package 可以作为伴随参数；叶子 Store 仍受已有捕获资格约束。输入与输出的声明模式沿真实 LocalId、ValueId 和捕获位置传播。两个值即使具有相同 ClosureId，也不共享一次消费责任。实际 Fn 传给 `F:FnOnce` 后仍必须按声明的消费模式使用。

这是普通拥有边界，使用 CallableReturnContract，不伪造空的 noescape helper 合同。Analyze 与 Verify 分别从实际签名、调用、MakeClosure、返回和捕获图重建资格；Verify 不调用 Types/Analyze 的求证函数。既有全局布局、SSA 责任、借用和递归清理仍继续检查。

当前不开放多 F、隐藏在 State/Package/Store 中的额外 F/G、借入 F 的长期保留、普通 T 混合、receiver、dyn 或一般 lending 根签名。既有有限 Copy 函数指针/ForeignCallback 沿原形状和调用规则保留，不增加 FFI 权限。已有合法 lending 子闭包可留在环境内部，但工厂根签名不因此扩大。

公开 schema/Wire 保持 **27**，闭包内联布局和已有清理 ABI 保持；没有为了通过新源码增加隐式堆盒、RC、锁或全局指针表。

具体前端来源见 [LANGUAGE-218.md](LANGUAGE-218.md)，独立审查见 [联合审核摘要](../evidence/reviews/218-221/by_language_parallel/SUMMARY.md)。

### 声明和语句列表中的源级 `#if`

219 实现真实编译期条件选择。条件可以调用无条件、非泛型、无捕获、已封闭的普通纯标量函数，以及它们的有限依赖；Unit/bool/u8/u16/i32/i64、循环、递归和短路沿已有 CTIR、独立 CT verifier 与受预算 VM 执行。

所有条件读取同一个不可变 ConfigReady 快照，保留无条件函数的原正文及普通 public import。条件生成的声明不会反向加入这个快照；依赖尚未就绪的生成声明、正文或形状时明确报阶段错误。失败条件不会被当作 false，也不会回退到运行期。

两支都做语法检查，只有选中支进入后续普通语义检查。选中的声明或语句按原位置插入，大括号不产生额外运行期作用域，因此生成的局部绑定可以被后续语句使用。选中程序仍经过 Types、Analyze、Verify 与机器生成。

选择与后续 `#(...)` 使用同一累计 CT 步数/节点余额；缓存保存原始结果和实际消耗，命中仍扣费，且每个成功请求仍执行当前独立 Verify。`selection.json` 保存依赖、源位置、选中支和消耗，它不是可注入的安全证明。

`#for`、`#emit`、完整 Type/Syntax 与卫生生成、任意 CT 全局/反射、泛型 CT 值参数、条件 import 等仍有各自未完成责任。详细语义和作者原始收据见 [LANGUAGE-219.md](LANGUAGE-219.md)。

## 独立审核实际发现并关闭的问题

| 问题 | 修复与复核 |
|---|---|
| `(模块别名).函数()` 的依赖扫描漏掉括号，合法调用的轨迹为空，条件生成 callee 被误报普通未知名 | 逐层解开 Group，先扣扫描预算，再沿原词法/导入/可见性/阶段检查。原四份源码逐字复验，依赖、错误码和准确 span 全部符合冻结预期。 |
| 捕获的 `F:FnOnce` 在返回闭包内经 `if` 结果转给局部后被误归为 Fn，造成合法源误拒 | 在两个值分支原有推导/统一后，沿真实捕获位置记录拥有转交并预付查找费。原误拒与三个正控通过，二次调用仍拒绝；原正例 O0/O2 实际运行通过。 |
| 新 raw/Wire 工厂伴随参数可能通过直接 Store 资源隐藏额外 callback | 独立 Verify 补足叶子 Store 资格；原误接受和同输入修复记录保留在 221 作者证据中，未以放宽前端或共享证明绕过。 |

前两项发现分别是依赖/诊断缺口和合法源码误拒，没有据此声称已经证明 UB 或过时缓存执行。原源码、oracle、失败结果、修复提交和复验身份全部分开保留。

## 正式整仓验收

[GitHub run 41057841800](https://github.com/zil-lang/zil/actions/runs/41057841800) 全部成功。GitHub 实际检出其 PR 合并提交 `fc5b3c058c1f4c112cbadffdb4f9f41705049ea6`；其完整 Git tree 是 `cc9f3580cb2dcce2b51f037a37acb87b1d8365b2`，与受验组合 `69931f83` 完全相同。两个提交身份分别记录，没有互相重标。

| 既有验收门 | 实际结果 |
|---|---:|
| 默认完整 workspace | **2817 通过、0 失败、0 忽略**；295 组 |
| `--no-default-features` 完整 workspace | **2785 通过、0 失败、0 忽略**；295 组 |
| 格式与 all-target Clippy `-D warnings` | Pass |
| 显式 LLVM text-reference | **36 通过、0 失败、0 忽略** |
| 迁移运行器自测 | **9 通过** |
| 既有 208 原生 API/State 消费者 | **16 次构建、128 次执行、56 次实际分配失败注入**，Pass |

工具链是 Rust/Cargo **1.99.1**、LLVM/Clang/LLD **23.1.4**。完整日志已压缩保存，结果、实际检出身份、逐步骤状态、计数和远端 artifact 摘要在 [CI 收据](../evidence/integration/221/ci41057841800/RESULT.json)。默认与参考配置有大量相同测试，不能将两列相加当作不同能力数量。

两次本地全量分别在停止/释放满盘缓存时中断，以及明确返回 StorageFull/E-ARTIFACT-IO；均不计完整 Pass。完整日志、源码清单、失败目标产物及临时文件摘要留在 [首次中断](../evidence/integration/221/infrastructure-abort/PRESERVATION.json) 与 [容量失败](../evidence/integration/221/infrastructure-storage-full/PRESERVATION.json)。成功中间样本的临时机器文件已释放，不作为正式验收证据。

## 限定证据的身份和计数

| 观察族 | 身份与结果 |
|---|---|
| 218 作者发布后复验 | `4d331dd6`：Expand 6/6、Types 15/15、精确预算 3/3；源码和工作树前后固定。 |
| 219 作者修复后交付 | `deb5824c`：54 项限定回归，100 次原生执行，其中 28 次实际分配失败，终态 live 全部为 0；证据交付为 `19375551`。 |
| 219 第二视角独审 | `deb5824c`：原四份输入 4/4；实际依赖、生成 callee 阶段错误、运行期遮蔽和源 span 均符合原 oracle。219 生产文件在最终组合中逐字相同。 |
| 工厂非作者新增输入 | `69931f83`：12 次源码检查、10 次 O0/O2 原生执行、9 次独立核入口，全部符合冻结预期。 |
| 工厂非作者重放既有 oracle | 同一最终组合：221 的 10 个测试组、42 次原生执行，其中 6 次实际分配失败，终态 live 全部为 0。与作者历史 42 次分开记账。 |
| 两条新增能力的交互 | 同一最终组合：`#if` 生成公共拥有工厂与后续局部绑定，Verify 和 O0/O2 实际执行成功。 |

同 ClosureId 的两个独立 ValueId 被交换时三个核入口都接受；保持布局而重复同一 ValueId 时，Analyze、raw Verify、Wire Verify 都拒绝。拒绝依靠真实责任检查，没有借失效的 plan identity 或 ResourceLimit 过关。

旧 helper、State、普通 opaque 三源的源码、Core/Typed 字节与精确预算保持：

| 旧输入 | Expand N | Types N | Analyze N | raw/Wire Verify N |
|---|---:|---:|---:|---:|
| helper | 586 | 421 | 1464 | 1908 |
| State | 829 | 1441 | 4701 | 7043 |
| 普通 opaque | 3 | 462 | 3960 | 5479 |

新条件转交输入为 Expand 818、Types 978、locals 14、functions 5；新源派生工厂 raw/Wire Verify 为 16559。N、N−1 与 fresh N 均有实际结果。数字只描述固定输入，不是默认额度或性能阈值；默认预算没有增加。

## ZUI 前置的当前判断

[最终 readiness 收据](../evidence/integration/221/readiness-final/READINESS.json) 重放了原 217 的 29 份源码和原 oracle：**13 项成功、14 项 Unsupported、2 项原语法无效**。只有声明级和语句级 `#if` 相对原观察晋升；两份无效语法继续排除在能力结论之外。公共工厂的新鲜包装接口另有本批真实源码证据，不能把旧“原 F 原样返回”探针的拒绝解释为本批未实现。

此前已核的普通同步 Node/F/G/Store 消费者继续成立；它仍是初始三个节点、固定容量满拒绝、O(n) 删除的具体实现，不能替代全部图规模或产品性能结论。历史 217 的运行次数、容量与拒绝边界未改写。

| 剩余方向 | 当前缺口及其影响 |
|---|---|
| 更自然的拥有数据与容器组合 | Product 进入 Store、Product 持有 Store、嵌套 Store、拥有 Sum/Option、Product 错误载荷等仍未全部开放；既有只读 Product 借入和更一般借入保存也未由本批完成。 |
| 一般高阶与开放异构 API | 原 F 原样返回、拥有参数/结果的通用 callback、关联项/life/HRTB/dyn 等仍有未实现范围。有限静态 F/G 家族不能冒充任意运行时 handler 集合。 |
| 声明式 ZUI | 当前 `#if` 是已完成的一步，完整 Type/Syntax、受预算生成、卫生、映射和生成/手写同核仍未完成。 |
| 外部与并发生命周期 | 长期 userdata、重入、注销排空/no-future、async/取消和并发责任需要单独闭环，不能由同步 noescape/static FFI 的成功代替。 |

这些是保留的真实语言责任；是否属于某个具体 ZUI 公共 API 的开工阻塞，应由该 API 的最小源码确定。本批没有删除用户的声明式、开放异构或外部生命周期目标，也没有用一个有限同步消费者宣布全部前置完成。

## Rust 四目标

安全方面，本批具有真实源、独立核、最近反例和机器失败清理，覆盖上述有限 profile；它不是完整语言无 UB 的最终证明。运行方面保留现有表示并核对分配平衡，尚无同语义最佳 Rust 的运行时间/RSS 对照。表达方面现在能写公共拥有包装函数和编译期选择列表，尚未完成完整 UI 演进与维护成本比较。编译方面已有累计预算、缓存身份和旧精确边界复核，尚未证明相对 Rust 的冷编译优势。

本批是可运行且已验收的语言增量；Rust 四目标合取及至少一项产品级实质突破仍需独立证据。原生 Windows、产品比较和完整 ZUI 不随本批测试数量升级为 Pass。
