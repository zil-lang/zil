# 全流程检查点：328

冻结编译器 `328eb392d8d88148a75a743d899b43aebae73587`，captured main `3ec7e8a960ca69368cd9a90f1a15ac063fec5a02`。源码、完整门、全部观察和首败见 [机器可读收据](full-push-checkpoint-328.json)。本轮继续施工，不关闭显著超越 Rust、全局最简或完整 LLVM 增量化目标；研究开关默认关闭，不合 main。

## 当前逻辑

变化输入→当前实际依赖绑定→受影响函数重算→值相等保留产品身份并停止传播→独立 Verify 自己绑定当前输入/证明/目标/完整预算→机器函数与 CGU 实际消费者→对象相等停止→当前完整链接输入相等时恢复 exe。方向是 Zit 的逐层 push，消费者已通到最终产物；外层整项目进入、全局聚合和中央 ReactiveGraph 未全局接管仍存在。Rust query 从需求向下核依赖的 pull 与之方向不同，但同样要用结果等价停止传播，仍有全 crate 与 affected-CGU 工作。固定 Rust 源码引用见 [既有审计](pipeline-audit.md#official-rustcargo-fixed-source-comparison)。主测 ProducerMemo 不运行 signal 读取 pull，不能通过删另一条读取路径冒领提速。

## 实际删除的工作

- **CT 当前余额与完整合同分离。** 真实425函数新增局部变量使 Expand 多扣5个节点，旧缓存把822486→822481的余额变化当作用户规则变化，重查全部425。现完整用户 Limits 仍严格相等，当前私有 SourceProgramContext 仍精确绑定 current program+effective Limits；只允许已扣 nodes/CTsteps，原 shape/body 单调费用逐笔扣，余额不足回原首错路径。实际 alias 断言21检查/404复用已过，[Astra 费用闭包](full-push-astra-ct-allowance-review.json) 保留最近反例。
- **类型 body 与 raw placement 分离。** 私有 `FunctionBodyRevision(Arc<()>)` 只由原始成功 lower 的完整字节/环境/生成名称证明铸造，非公共hash。错误跨epoch桥接全表核当前上下文/声明owner/namespace，NodeId改变即使span未变也更新binding；Typed FunctionId仍是声明槽位。真实425 alias-error→recovery达到1body+20position+404retained、完整fresh Eq及本次Verify1。[Astra 源码复核](full-push-astra-body-placement-review.json)。Expand后续Core重建和gap全表扫描尚在。
- **LLVM完整AU只存一次。** 当前 scheduler 四列有序值逐项相等且实际FPPM slot匹配后共享不可变表，执行序列为8B ordinal/flag；删除完整向量的临时深复制及重复保存，provider闭包/64MiB原上限/完整MachineVerifier和typed/MIR比较保留。最终计量按实际强owner去重，不称RSS。[实际源码独审](full-push-native-sharing-link-review-addendum.json)；此前未完成审查首稿也保留。
- **真正拒绝缓存时不建无用FAM集合。** 普通TimeOnly/Bypass/无效时钟路径不枚举当前分析集；实际cache key/capture/replay仍取完整当前entry/support/facts/Input/World/fresh pipeline，诊断模式保留原行为。每个effective slot仍推进，不改变PA、AM或原O2 passes。[独审及实际访问反例](full-push-deferred-analysis-keys-review.json)。
- **链接与写出。** 两个当前工具ldd合并为一个严格双section探测，不支持时完整回退原两次；库内容、目录负依赖、环境及链接前后绑定不减。完整诊断文件使用有界1MiB缓冲；无修改早已复用诊断分区，没有虚构425项聚合删除。

## 已核原负载，负结果不删

恢复环境后重新运行的 profile12 是328/main3ec7同一窗口顺序交替的**一个诊断block**，425函数、22文件、20×20×16、CGU16/O2/jobs1、默认预算。候选带profiling；这不是正式Rust对照、跨版本收益归因或统计领先证明。

| 请求 | main ms | 候选 ms |
|---|---:|---:|
| cold | 5725.460 | 6944.966 |
| unchanged | 487.285 | 75.741 |
| private | 1453.001 | 461.152 |
| multi | 1675.551 | 698.799 |
| broad | 5816.821 | 6628.490 |
| positions | 1455.649 | 838.316 |
| error | 374.026 | 57.392 |
| recovery | 1815.598 | 568.293 |
| restore | 535.013 | 176.114 |

冷编译和广泛修改仍负。private实际39MF复用/3执行、2FPM复用/376执行；object scope优化157ms、发射68ms，完整请求461ms。cold实际424MF/3816FPM原优化执行，object scope优化1469ms/发射2623ms；MIR key95ms/捕获295ms，仍是后续成本。嵌套计时不相加；完整阶段范围见 [profile12](full-push-native-profile-12-scopes.json)。冷逻辑retained bytes41,292,826不是RSS或整个进程严格界。

[profile12资格](full-push-profile12-qualification.json) 核完整34rows/18测量16父历史/17pairs，其中15完整ELF记录与严格LLVM原始blob对照、2safe-error pairs、固定Git generator重算180原生结果，实际binary与DSO/map/runner/helper/source/summary均核。旧exe被后续请求覆盖时不冒充重新物理hash所有历史exe。

[profile10](full-push-profile10-qualification.json) 另绑定25c，不将其471ms与这里461ms拼接归因。环境重连后的 [profile11](full-push-profile11-qualification.json) 虽完整质量通过，外层controller句柄已丢，时间连续性未知，**不用其时间提速归因**；原始数据保留，12是独立新运行。

## 验证、保全与未完成项

按整个组件改通后统一测试。25c：CT74/Expand102/Types103/driver210、strict Clippy12/fmt12/release12过；随后只改work_products.cpp，328：native72/backend242/mixed3/strictClippy13/release13过。未重跑当前完整workspace，重叠范围不相加。原driver14为208/1，未放宽21/404断言；临时诊断确定5node原因后还原，原lock路径缺失退出66也明确是未执行测试的基础设施失败。旧990专用validator拒绝新3ec/push属于工具pin不适用，历史工具不改；新只读qualifier独立严格核原始证据。

当前正式六轮对照已完成，见 [328全部八场景](full-push-matched-328.md)。两模式各270请求，O2共2808原生执行，78质量对（42测量+36父准备），479 blob核验。单处555/Rust706ms、多处729/1022，但cold7866/6881、broad7012/5821、noop63/15仍负；恢复中位数527/565而配对几何比1.092，不能宣称全面领先。原生请求CGU16/O2/jobs1匹配，实际分区内容与LLVM版本/检查保证不同。完整原始结果分别在 [v17](full-push-recovery-v17.json)、[v18](full-push-recovery-v18.json)，三个profile及门在 [v16](full-push-recovery-v16.json)，固定远端逐片全文件hash/模式、原始manifest完整不相交全集均fresh HTTPS核验；没有宣称整份拼回后再次运行validator。原validator在本地payload退役前通过。首次失败与旧环境原始丢失边界保留。

尚未最简：整项目阶段key和全局inventories/fingerprints、Expand后续Core重建与gap全表核、完整诊断/对象IO、当前链接发现、受影响CGU全局LLVM工作、MIR key/capture及warm重建验证均在。均衡拆CGU会改变body可见性/内联，未证明质量保持，因此 [不实施](full-push-balanced-partition-negative.json)。Astra的 [LLVM发射事务审查](full-push-astra-emission-transaction-review.md) 仍有MC/AsmPrinter状态闭包未证明，约40ms warm毛收益上限不是全部O2终态。后续保留原生owner的方案正在核实际寿命/符号状态，未写成已实现或已测成果。
