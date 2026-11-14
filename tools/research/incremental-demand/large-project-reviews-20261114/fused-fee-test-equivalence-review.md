# Fused fee 测试后继：精确独立等价性审查

**裁决：ACCEPT（5517 测试合同修正及生产源码等价性）。** 未发现通过取消真实费用、放宽 temporary peak、接受 N−1 成功或删除现有语义 oracle 来掩盖失败的修改。原生产 e005 的源码 ACCEPT 复用；本报告不将原 f7 失败覆盖为成功，也不预先接受尚未冻结的 Clippy typealias 后继。

## 精确身份与只读范围

- production：`e005777aa63d8a3031a9024eaa12af5376d6904c`。
- 测试基线/直接父提交：`f7b9a6a6d37837f42fa87aa2ea5dbdd283f8cc66`。
- 本次 source：`5517e70847dbf89933d3412066ba2f116f2220b1`。
- 本次 tree：`d5114048609bf1b291c4c2b2d29afbda837c1100`。
- f7→5517 canonical patch SHA256：`e0a08294fda8a114994273106d5e8d3ca4186d410bec37a4f52672e22185fc3d`。
- e005→5517 canonical patch SHA256：`d719bbe1371c7f127c92e41207767986a859cb82e73789fc3606e5dc5a13ca29`。
- 原生产 base `ad81486b976634e71bf1c61544d0fd28a783fbae`→5517 完整 patch SHA256：`22634590927e1e2ad5380e5a8811e93ea5e0f77d68650204bad8933840995d50`。

统一哈希格式：

```sh
git diff --no-color --binary --full-index --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/ BASE SOURCE | sha256sum
```

实际差异为三文件、286+/109-：`ct_artifact/scalar_materialization.rs` 的 cfg(test) 模块、`producer_reuse_tests.rs`、`project.rs` 的 filesystem 测试模块。producer_reuse_tests.rs 通过 session.rs 的 cfg(test) mod 引入；另外两处带生产代码的文件，变更 hunk 均位于测试模块。未改变生产入口、默认 Limits、Verify、compiler oracle 或原 source fixture。

另外逐字节核了两文件首个 cfg(test) 前的生产前缀，f7→5517 完全相同：

- scalar_materialization.rs：13297 bytes，SHA256 `1817f2b444d578d2b5027f018a3deba84bb10129a593330463bc46b24e3e536f`。
- project.rs：108217 bytes，SHA256 `de6e1dbfaf8d80e2872384858f9f274c107e5e9d572728d7f0da3f64d86ce0df`。

f7 相对 e005 已确认也只有 Expand 测试修复，故生产代码相同结论传递成立。不同 source 身份或构建配置下不宣称整个二进制文件字节相同。审查中存在另 worker 未跟踪研究 evidence 目录，不纳入这份已提交源码裁决。

本次读取精确 diff、测试/helper 上下文、必要原调用边界及原失败日志；未构建、未执行测试或测量，未修改 tracked/Git。生产语义/实际收费的先前报告 `/workspace/zil-recovery/scalar-admission-exact-review.md` 保留不覆盖。

## 1. Raw 锚没有被替换掉

新增 `raw_capture_fee_anchor_and_current_fused_savings_keep_complete_scalar_outputs` 使用实际 `materialize_scoped`，明确固定 raw capture `(work,nodes)=(137,68)`、raw 总 usage `68 nodes / 1006 work / 1280 temporary peak`。不是将 old raw 的期望数直接改成新路径数。

它保留完整原 ParsedSource fixture，要求 raw modules 与每个原解析 module 完整相等、数量相同，并无 runtime environment、preview 或 required imports。新 fused 则经真实 ScalarMaterializationSession 入口取得，并固定为 `5 nodes / 883 work / 1280 peak`。其余 CT steps、call depth、live cells、instances、queries 与 raw 一一相同；node/work 剩余量也分别严格多 63/123。

这与独立费用核算吻合：旧路径三 module 加完整 Discovery 为 68 node、69 字符工作，共 137 work；新路径三 module 加两条 namespace row 为 5 node，`left`/`right` 字符工作 9，共 14 work。实际后续 materializer 均为 869 work，故 137+869=1006、14+869=883。新测试明示该等式，没有靠虚构 receipt 扣除差额；1280 峰值没有减少。

## 2. 当前 fused fresh 是费用 oracle；decline 仍走真实消费者

原来四个失败测试将 fused usage/remainder 强行与 raw 域相等，或用 raw 的 N−1 要求 fused 也失败。修正后，`fresh` 使用新的独立 ScalarMaterializationSession，但调用同一真实 materialize 入口；`same_current` 比较完整 CtUsageReceipt 与完整 Limits remainder。

`boundary` 搜索真实 `Ok(Some(...))` 的首个预算，不把 `Ok(None)` 当成功。`failure` 对 Err 保留完整诊断；对 None 调用原 `full/materialize_scoped` 并要求该实际 fallback 拒绝；若 Some 则直接 panic。因而没有把 classifier decline 转成假错误，也没有把未选择优化当通过。

该 helper 只用于 scalar materializer 阶段，full 确为 driver 的对应 ordinary fallback；后续完整 Expand/Types 并非这个 helper 的返回值。整链预算另由 filesystem 测试经 compile_path 覆盖，不冒称这个局部 helper 代替了完整 Verify。

## 3. Node/work/temp/depth、失败发布与恢复

- **node 与 work**：各自搜索 fresh fused N；N 的当前结果与 fresh 完整 usage/remainder 相等；N−1 与 fresh 的完整诊断向量相等且非空。work 还要求 N 等于真实总 analysis_work，保留 receipt miss=1、clone=1。node classifier 可能 decline，按真实 fallback 校核，不伪装成 receipt miss。
- **失败状态**：node/work 的每次失败前后 entry 数量相同，逐 entry Arc 身份不变；恢复 N 要求完整结果恢复、三个模块复用。最后还与最初 generous checkpoint 比较 entry Arc。没有把失败尝试的部分 entry 当作新 epoch。
- **temporary**：独立搜索并固定 N=1280，usage peak 也必须恰 1280；1279 与 fresh 失败一致，保留 miss/clone 控制及失败前 entry Arc；恢复 1280 后完整 usage/remainder 恢复且三个模块复用。没有借费用域变化放宽实际内存峰值。
- **depth**：独立搜索当前域 N；从宽域转入时必须 reused=0，下一次 N 必须 reused=3；N−1 对完整 fresh 诊断、reused=0 和旧 entry 身份，随后恢复 N 的完整结果与三个复用。

未将“与 raw first error 不同”作为自动失败；保留的是各真实费用域内 cold/warm 的当前首错及完整诊断相等。该修复没有更改 production 是否原子发布的实现，新增断言直接观察其已有状态。

## 4. Filesystem 整链同时保留两个费用域

`producer_filesystem_ordinary_emit_preserves_materializer_budget_n_n_minus_one` 的完整三源输入（含原 IO fixture）未改，仍 stop_after Core。先独立搜索原 raw N，并在同一 raw session 中跑 N→N−1→N，继续使用原 `compare_path` 对照 fresh raw 的 status、phase、完整 diagnostics、fingerprint、source_files、interfaces、Verify 次数和 active-query 清理。

随后在 graph=false/true 下各搜索 fused 自身 N，要求 `0 < fused_n <= raw_n`，且 work 的 fused_n 严格小于 raw_n；N 的 cold 结果必须确实有 admission hit 及 fused distributed items，不能靠退回旧路径伪装新域通过。

同一 fused session 执行 N→N→N−1→N，每次与独立 cold fused 的同一限制比较上述完整报告字段，N−1 明确必须 ResourceLimit 且诊断非空；成功仍与原 generous raw 结果的 diagnostics、fingerprint 和完整 source_files 比较。另保留 raw N−1 在新 fused 下可能成功的观察，结果由两个独立测得边界决定，没有把它删掉或写成无条件 Success。

**准确的覆盖边界**：这个 Core-stop 预算测试原先的 compare_path 本来也只在 stop_after≥Types 时比较完整 TypedProgram，故原此处没有 Core/Map 的直接等式；新测试不能宣传为新增了该等式。全文件其余完整 TypedProgram/Source/error 对照、Expand 的完整 Core+Map 测试和 current Map 验证均未改动，继续复用既有源码审查。Core-stop 的 Verify=0 是原阶段合同，修改后对 current/fresh 都显式核 0，不是将本应执行的 Verify 降到 0；其他完整检查测试的 Verify=1 断言没有删除。

## 5. Parent-return 只改实际工作归属

project.rs 仅把旧 `classified_modules=1` 改为 `admission_classifier_hits=1`、`classified_modules=0`、`classifier_nodes=0`。它没有仅把“做了工作”改成零，而是核当前 changed owner 的分类在实际 admission 中完成。

resident parent 的成功/Verify=1/parser=0、changed source 与 fresh 的完整错误和具体 absent_name span、失败 Verify=0、parser execution/reuse=(1,2)、Lower=(1,2)、imports 未变、materializer=(1,2) 均原样保留。session.clear 后 weak epoch 释放断言也没改，没有拿 recency/history 新 owner 掩盖旧 parent 状态。

## 实际门与结论边界

已实读的原 f7 driver.log 为 207 passed/6 failed/0 ignored；SHA256 `a77ae5133b4b0b4d7472b8f41f974c54b73db24df251035caa88bbeee9126619`。六处失败对应四个旧费用同值假设、一个旧域 N−1 对比及一个分类归属计数；该原窗口保持失败，不改写为补丁后的通过。

root 随后报告 5517 materializer7、budget1、parent1、Source9 已过，native publication1 在补齐环境后通过，并保留首次缺 ZIL_TEST_NATIVE_LINK_PROFILE 的失败。本人没有运行这些门或在此逐项核对新 raw，故这些为 root 回报，不能据此宣称已重新执行完整 driver/source 库。

root 另报告 strict 仅遇新增 Expand 测试 tuple 的 type_complexity，正准备同字段类型的 FixtureInputs alias 后继。本报告只接受已冻结 5517；后继精确 source/差异另绑定，不提前记录 strict 或全部门通过。

**确认需要修复的源码项：无。5517 的测试适配接受；e005/f7 生产源码裁决完整复用。原 raw 锚、真实 fallback、fused 各预算边界及失败恢复没有被取消。**
