# 施工10：225/226/230/231 有限语言主线验收

四片已经经非作者审查和独立的 PR/main 完整 CI 合入。共同受审实现为 `75d08022213a5da96e1968d7fe7d1d1e341b5907`，通过 PR 验证的候选为 `f1492652cb6497a33f39e615fd398456ce09253a`，实际 main 合并为 `9e7072b1ebdc6900c7370863c27c7432953bd830`。正式工单与聚合结果见 `.task/impl/225.md`、`226.md`、`230.md`、`231.md` 及 [RESULT.json](../evidence/integration/construction10/RESULT.json)。

## 本轮能力与保留边界

| 切片 | 本轮接受的有限能力 |
| --- | --- |
| 225 | 自含环境中的完整 Product Store 拥有捕获，保留实际责任转交、布局与清理。 |
| 226 | 已证明普通局部根的显式同步共享 &Product 捕获，最后实际使用之后才允许转交或替换。 |
| 230 | 有限 #for 中生成无泛型 Unit/Tuple enum，支持名义卫生、构造/匹配、同批前向绑定及普通复检。 |
| 231 | 普通自由泛型函数既有名义 trait 的 where 正规化，保留模块身份/隐私、Copy 组合与保留能力边界。 |

226 仍只开放已证明普通局部来源的显式共享捕获；默认只读普通 Product 捕获、incoming parameter/capture 来源尚未开放。拥有 Store 的环境仍必须自含，不能在同一环境加入外部共享 Product hold。两个同时存活的局部环境已有真实组合与清理证据。

230 尚不开放泛型/record enum、生成 trait/impl、一般 Type/Syntax/#emit 或资源能力。231 复用既有名义约束，不实现一般 trait solver、associated types、generic supertraits 或保留能力。trait 声明与 impl 分处不同模块仍 Unsupported；新的三模块消费者保留真实模块值流，但让声明与 impl 同处其定义模块。泛型方法调用作为 enum payload 的未标型推导仍有既有边界，显式 i32 对照已通过。

## 实际验证身份

| 实际受测提交 | GitHub run | 默认 passed/failed/ignored | reference passed/failed/ignored | 工作流步骤 |
| --- | --- | --- | --- | --- |
| PR 实际检出 `045dc75cec6eaa4de52951e075c52fd809fb6b3c` | [41184530698](https://github.com/zil-lang/zil/actions/runs/41184530698) | 2976/0/0 | 2933/0/0 | 25/25 |
| main 实际合并 `9e7072b1ebdc6900c7370863c27c7432953bd830` | [41186163758](https://github.com/zil-lang/zil/actions/runs/41186163758) | 2976/0/0 | 2933/0/0 | 25/25 |

两轮各配置均零过滤。fmt、Clippy、text-reference 36/0 和 migration runner 9/0 均以实际原件核验。每次 State/分配故障消费者分别为16次构建、40个 LLVM API builder 对象、128次执行、88次 allocator 观察、56次真实失败注入，全部观察最终 live=0；145条command记录不是145次程序执行。不同配置与重叠重放不相加成独立覆盖。

三位非作者对四片完成26项独审（225/226=9、230=9、231=8）。PR 默认后端另核10+8+8次实际 O0/O2 LLVM API 23.1.4执行；reference 的对应26次 Clang执行独立作为同源对照，全部无fallback。main的精确输入、工具链、具名测试与上传库存另行核对，按不变输入复用详细PR原生审查，并不声称重新逐件下载、审查main大包。

230独审 work3014/nodes512、作者2698/871，231新三模块独审 work2169/nodes394，均核冷/热/fresh、N−1拒绝及失败后恢复。计费观测绑定各自输入，不能从输入变化推出编译提速；旧231预算4410/922不用于本轮。默认额度与产品阈值原字节保持。

## 失败、恢复与记录完整性

首轮run41182685056仍是Fail：默认2967/6/0、reference2924/6/0。原同环境混合捕获、raw注入定位、旧Marker期望和231接收者/跨模块impl/enum推导夹具失败均保存原输入与原报告。后续修正只调整有证据的测试定位、既有边界对照或旧合法输入期望，没有为绿灯放宽生产安全检查。

225/226原远端tips真实保留为main祖先。230/231原生产和既有测试按Git blob恢复；缺失的旧interfaces尾部按同一合同重新冻结，缺失的231模块例子作为新输入重新执行。原临时工作区丢失及未取回的历史证据见 [RECOVERY.json](../evidence/recovery/construction10/RECOVERY.json)，不可据当前通过重写旧缺证。另建的独立沙盒拥有完整自有对象库、无alternates并通过strict fsck。

完整工作树的收口前handoff基线仍Fail/1145 errors，四个本批待验切片各0错误；该记录完整性门与编译器CI分开。最终实际检查及差分另记收口收据，不把旧1379/1381或稀疏检出时期的结果改写成Pass。

## Rust四目标与下一步

有限来源/权限、真实清理、raw/Wire反例及模块卫生有本轮实证；完整语言安全仍未建立。内联表示与真实分配平衡没有证明同语义Rust运行/RSS同级，表达组合增量没有证明维护成本优势；本轮没有编译性能采样。四目标合取和产品级可重复净突破未建立，产品条件不变。完整1.4/ZUI保持NotSatisfied，B0 Doing、181施工，产品与原生Windows NotRun。

下一优先是普通局部根的默认只读Product捕获：必须在真实使用/拥有分类后选择借入，并保留子字段消费与写入责任。227普通defer继续明确注册动作的必须执行义务、五类退出及逆成功初始化顺序；FnOnce最多一次不能代替必须执行。编译性能/增量研究支线保持独立，其未合提交不清理。
