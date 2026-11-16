稳定 class → CGU → object：方案与施工边界审阅

2026-11-16。基线 `df06acabf33a76a723d1e92b1d7da71b41d4b7da`；读取的 backend/stable-source 源码在 4a23→df06 提交间无差异。随后按 root 指令审阅当前工作树四文件修改：machine_call_classes.rs、build.rs、incremental.rs、machine_cgu.rs。未编译、测试、计时、改源码或原始收据。此文记录方案及本次读到的未提交版本，不是后续冻结提交的验收。

**首选方案是 root 已施工的 body-derived 稳定 class 实现：完整 normalized Function Eq 决定类，指纹只决定实现符号；保留全部原定义和地址行为，保留 complete imports、逐 unit prune、full O2。** 不优先选择 caller-owner 局部代表：它缩小等价合并/跨单位优化范围，会改变原质量策略。也不建议只把代表从“源序最早”换成“稳定源码名最小”：被编辑的恰好是该成员时，代表仍会更换，原问题仍存在。

基线的具体传播链是：`machine_call_classes::canonicalize` 按 DAG 高度、源序选择代表；`normalized_key` 用代表的 `zil.fn.<id>` 规范化调用；全 module 的 direct calls 随代表变化。`machine_cgu::partition_view_once` 又为每个 owner 导入完整 direct-call closure，实际 private import 名包含 owner 与被导入函数名。`object_cache::machine_key` 哈希当前 triple 和完整、有序 items，因此符号、引用、导入实现或顺序的真实改变都会改 object key。4a23 的 15–16 misses 证实失效范围，不能仅凭计数证明每个 miss 都只有这一原因。

稳定 class 使一个成员退出原等价类时，剩余类的实现名、相同父类 key 和实际导入函数名保持不变；真正调用被修改成员的函数仍应获得新 callee/class，继续触发真实依赖传播。这是消除代表身份造成的无关变化，不是让真实依赖变更继续命中旧对象。

**当前实现已接对的部分。**

- `normalized_key_inner` 在 stable 模式下规范化所有已分类 callee，包括原代表本身；不再把代表源码名漏进父类 key。HashMap 的完整 Function Eq 仍决定类；相同新符号对应不同 key 时拒绝整次暂存优化。仅 External 的纯 DAG class 进入新分支；原 protected/address/ABI/不支持操作边界保留，cycles 及依赖 cycles 的 caller 不获此优化。
- helpers 按稳定符号的 BTreeMap 顺序追加，保留原定义；原地址操作数不被重定向。新增定义的 linkage 为 External，允许当前 CGU planner 决定实际 owner，之后才做 closed internalization；不能提前设 Internal 把跨单位引用强行合并成一个 linkage component。
- `TransformProduct` 同时保存真实 transformed machine 与 class_symbols；active/completed 恢复取同一产品，原 identity/TransformPolicy gates 保持。当前私有 BTreeSet 并非从磁盘/名字前缀构造，且 planner 核对实际 External definition；在这条封闭接线中不需要再造持久授权框架。若以后拆开这两份数据或单独接收外部集合，必须重新建立绑定。
- `partition_classes_reusing` 用当前完整 planner，源定义分组不变；非源码 helper 的 owner 来自其稳定名字 hash。只扩展本次实际 importable domain，原 source_functions 仍用于计算 source-cost 基数，未通过 helper 增加 import 额度。
- 新 domain 暂不读写旧磁盘 layout receipt，是合理的最小边界。旧 `layout::Layout::valid` 独立限制 LIR source 名及 copy cost，不能只改 planner 就让它接受新域。当前 Materialization 则在本次 planner 决定实际 roles 后构造；其 full item Eq、owner、role、import-name 依赖检查仍在，复用不是跳过当前规划。
- helpers 加入 internalize candidates，仍由全部当前 units 的引用决定可内化对象；随后逐 unit `machine_closed_prune::prune` 在非空删除前验证完整未裁剪 unit。已有 prune reuse 要求完整 triple/items 相等。后续 API preflight/Builder/O2/LLVM 检查没有被代替。

**本次 diff 需要收口的三个具体问题，已直接反馈 root。**

1. `partition_classes_reusing` 目前调用通用 `partition_view_recorded_reusing`。后者遇到 `complete O2 call closure exceeds total copy cost`，对仍带 helper 的 module 再以 imports=false 规划。这个既有回退本身不等于语义错误，但新 helper 域更容易触发，违反“本轮完整 imports 不减”的目标。新域应把该失败向 build 返回；保留折叠完成、尚未 class rewrite 的 module，丢弃 stable class/materialization 暂存，再跑旧 canonicalize＋原 planner。旧策略自身仍可能按其原规则回退，报告必须如实区分，不能把新域关闭 imports 当作稳定 class 成功。取消不是可吞掉的优化 miss，仍直接返回 Cancelled。

2. `canonicalize_stable` 当前在 rewritten_calls=0 时也追加所有 class helpers，令 class_symbols 非空，build 因此跳过原 `machine_stable_identity::prepare`。对所有普通调用已折叠掉的小 constant/call-free 程序，这会丢失原先只保留 entry/main 的稳定 DCE 路径。最小修正是在没有任何实际 call rewrite 时不提交 module/helper，返回空 class 集；旧 stable prepare/prune 自然继续。更一般的“原 stable prepare 已能完整处理”的输入应优先保留原路径，不要删除其 admission 门槛来适配新域：原 prepare 只允许 wrapper→source entry 的内部调用，其 prune 也专门依赖这一事实。

3. 新符号碰撞检查的 `names` 仅包含 Function 名，未包含 `Item::Global.name`。合法原 module 的 global 若恰好与新 class 符号同名，会在提交新 module 后变成重复符号错误，不能兑现碰撞时回退旧优化的行为。应对原 Function/Global 的完整符号集合检查，再与本次 implementations 检查。不能用“hash 不太可能碰撞”豁免这个确定边界。

这些是读取该四文件 diff 时的结论；未据此声称冻结版已关闭。

**费用、失败及依赖范围仍需明确。**

原 imports 的 copied_cost≤original source_cost 保持，是正确的必要条件；但 helper canonical definitions 本身是新增模型工作，copy cap 不变不代表总体模型扩张不变。每个完整类至多一个 helper，因此 helper 总 body cost 可由实际类代表的原 body cost 约束；仍应显式做 checked cost/item 累计并受已有实际模型处理上界约束，不把它加进 source-cost 基数。原定义即使后来 dead，其新 call 也会在 pre-partition 规划时诱发 imports；第一版可完整回退，不能临时截断 closure 或提前删掉尚未验证的定义以凑预算。

完整 class key 的哈希与 helper body 都是当前实际计算。哈希命名不能变成跳过完整 Eq 的依据；碰撞时必须在修改原 module 前退出，不用源序 suffix 补名。已保存的 class 产品不得反向持有 BackendSession/MachineProduct，继续沿已有四槽完成产物所有权保留，避免 history 环或额外保活。

当前 canonicalize 是 COW 暂存，失败前不提交；build 在 pass 后检查取消，逐 unit prune 也检查取消，driver 的 staged backend 仍只在成功后发布。新 planner 仍继承旧 planner 内部无 cancellation callback 的粒度；不能宣称新增分支改善了取消响应时延，但本次未见取消后可成功发布半链的入口。新域回退时新增尝试也是实际请求工作，不能从计时或统计中抹去。

稳定 helper 符号主要解决固定源清单中的“代表变了”；不自动解决新增文件、源函数增删/重排导致的原 `FunctionId`、FileId/ordinal chunk 变化。`SourceIdentities::bind` 已提供当前 Verify 后的完整源码身份映射，可作为后续实际符号/owner 稳定化入口；不能仅在 object key 中忽略 ID、排序或引用。原 planner 的 index-sorted layout 和 Materialization slots 都依赖实际 item 顺序，若后续调整顺序，必须同步布局/复用映射并向 LLVM 交付同一真实有序 Module。

当前实现仍全量运行 class 构图与 planner，不能据 object hits 增加便声称解决了 machine/partition 的全部 CPU 成本。先完成这条真实稳定输出链；后续仅可把当前完整 class/实际依赖产品放进已有 TransformProduct 做有界复用。仅凭单函数 lower hit 或旧 class 指纹不能跳过地址/导出角色、完整 policy、当前 caller/callee 关系与 copy-cost 决策。

**Rust 对照：借鉴稳定依赖范围，不导入它的授权机制。**

已读 pinned Rust `58cf8f9c35b355199727627d07ae34a4e2846d67` 的 `compiler/rustc_monomorphize/src/partitioning.rs`：以源码 module/DefPath 选择 CGU，incremental 下区分 stable 与 volatile generic；合并后的名称按组成 CGU 名排序构造。实现仍会按大小/重叠合并，并非任何编辑下 CGU 都固定。其 import/inlining 是 usage-map 标记的可内联项闭包，不能把 ZIL 的 complete imports 改成更小集合后称为同策略比较。

`rustc_codegen_ssa/src/base.rs::determine_cgu_reuse` 要求存在 previous work product 且当前 CompileCodegenUnit dep-node 能 mark green，然后按 LTO 选择 PreLto/PostLto。source body 改变会使包含该 body/inline copy 的 CGU 失效；签名改变会扩大到引用单位。ZIL 这里应保持真实新 Module 的完整 object key/preflight，目的只是减少无关 module 改动，不新增磁盘 Source→Machine 权限，也不把 Cargo Fresh 当成 rustc 正在执行的证据。

**冷/热版本范围核对：没有证据支持“当前整体冷更快，所以相对旧版跨窗反转”。**

| 已有版本与窗口 | 可证明的范围 |
|---|---|
| f19fc847 retain 六请求，两块，tracked cache-time-space.json | 5105 cold 已是 C 4.096152/R 7.182061 s；restart 后 private 为 C 2.772671/R 1.508134 s，落后。2500 unchanged 为 C 32.114/R 48.948 ms，领先；5105 unchanged 为 C 49.565/R 30.374 ms，落后。因此连 f19 自身也不能笼统称“热领先、冷落后”。 |
| bcf409b37 retain 的 restart-epoch-results-01 | 5105 restart 后 first edit C 1.2230/R 1.2854 s，可说该格领先；restart C 2.7251/R 0.0189 s、pair 3.9481/R 1.3042 s 均落后。不同窗口 Rust 自身也漂移，不能与 f19/4a23 直接作算法增益差分。 |
| 4a23 正式 retain | 5105 cold C 4.028748/R 5.686927 s；restart 后 private C 1.244866/R 0.995086 s。仅 first-edit 格相对 bcf 的 Rust 领先关系变了，不等于已证明候选算法回退。 |
| 4a23 正式 native 长流程 | 5105 cold C 4.594763/R 6.202751 s；第一次 private 位于 resident cold 之后，不是 retain 的 restart 后；八次 private 总账仍落后。完整 18 请求总账较低也不能称作 cold 时间或所有热格均领先。 |
| 4a23 check5000wide | cold C 1.621930/R 0.807345 s，仍落后；程序形状、输出范围不同于 native5105，不能合并为“当前冷更快”。 |
| df06acabf | 本次核对的 backend 与 4a23 相同；是结果/研究提交边界，没有新增该稳定 class 实现的性能收据。 |

以上原有窗口为 fresh 用户 artifact roots，不等于 OS page cache 冷启动；unchanged resident、restart、restart 后 edit、长流程 private 是不同场景。限定的 O2/CGU16/jobs1 相同也不足以消除这些范围差异。diag02 为独立阶段诊断，Verify 扰动及 Cargo Fresh 旧 time-pass 重放限制继续有效；此文不重算正式比率、不请求新矩阵。
