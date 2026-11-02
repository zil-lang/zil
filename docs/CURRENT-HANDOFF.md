# 当前接续入口：普通拥有Sum与同步语言组合

[PR17](https://github.com/zil-lang/zil/pull/17)已把239–244合入实际main `8b2ae1fdc88b4e1ae8dc73162b13a425d0f6f1ad`。共同受测/独审源码R为`15498f14af8f11d3a3bf4b198fe69208be5913ce`，两者整树相同。Rust/Cargo1.99.1、LLVM/Clang23.1.4，schema/Wire30；无需重装环境。

本批普通非泛型拥有Enum、Product内Sum、含Store Product错误载荷，接通构造、参数/返回、消耗式match、嵌套模式、整值替换、defer与真实失败清理。227/235/236/237/238此前已合入的同步defer、嵌套Store、完整泛型Product身份与替换继续保留。新普通资格不自动开放capture、StoreElement、数组、FFI或拥有泛型Enum。

本地默认3575/0、参考3474/0、文本参考64/0，均零忽略/过滤；严格Clippy、格式、State控制通过。239–243独审138/0，240独立补充61/0；五模块消费者476次native含252次真实分配失败，均live0。root另按源码独立推导预期复跑100次native（26次真实失败）；重放次数不叠加。

PR source CI41363882129、research LLVM CI41363882134均成功，各自默认3575/0、参考3474/0、文本参考64/0。常规artifact上传不可用，两份完整日志恢复归档已取回，root重核全部blob、4092个当前源文件及实际checkout b59b9f1a的整树身份。main自动重放41366807735/41366807646正在运行，未记为Pass。入口为[evidence/integration/owned-sum-20261102/ACCEPTANCE.json](../evidence/integration/owned-sum-20261102/ACCEPTANCE.json)。

P3548/12、Q全仓3575/0但独立合法owner被误拒、所有原输入及修复均保存。Q远端两次取消的可取日志已封存，其native归档无法完整恢复，不能计验收。历史handoff记录债务没有随编译成功清零，最终记录完整性另行核回。阶段总结见[本批报告](LANGUAGE-OWNED-SUM-20261102.md)。

下一主线是同步拥有Event/Reply经过具体闭包和公共generic dispatch的责任闭环；最终R检查确认三个拥有callback探针在Types拒绝，F原样转交在Expand拒绝，而已有State/Store对照到Verify通过。详见[下一优先级](LANGUAGE-NEXT-PRIORITIES.md)。完整1.4/ZUI NotSatisfied、B0 Doing、181施工、产品及原生WindowsNotRun保持；没有本轮Rust性能比较。

接续从主树AGENTS.md进入，运行handoff和check-plan。机器摘要、六张实施卡及当前交接已更新到本批身份，旧schema27摘要保存在本批previous-final-summary.json；源码事实仍以Git和精确收据为准。施工枝及原证据保留，没有进行分支清理。
