# 同步拥有回调：候选验收

共同源码 `288e29628389d83a335783d5488be39f22b2008b`（基线81b7ae0c）已通过本地正式验收及非作者审核，远端CI与主干合入尚待完成。精确交付、审核和结果见 [ACCEPTANCE.json](../evidence/integration/owned-callback-20261102/ACCEPTANCE.json)。

拥有Event可传入具体Fn/FnMut/FnOnce闭包，经公共单F按值FnOnce dispatch返回拥有Reply。Product、Enum、ErrorUnion与含Store数据走同一有限闭合路径；实际参数消费不提升环境调用模式。结果先准备，再清局部与Once剩余环境；晚到参数失败、回调body错误、拥有错误载荷、未调用销毁均有独立证据。环境ByteBuffer转入Reply与输入中的另一缓冲区分别回收。

Types、Analyze与Verify各自核资格和实际数据，Verify不调用前两核求证。修复了拥有参数移动后旧窗口误存活、helper被误限为原样relay、外层拥有ErrorUnion误拒、共享Product捕获越权及Store内隐藏F等问题。原误收/误拒保留，不以最终绿改写早期失败。schema/Wire30及内联机器ABI沿用，未加隐式Box、RC或锁。

| 验证 | 实际结果 |
| --- | --- |
| 默认全仓 | 3632通过，0失败/忽略/过滤 |
| 参考后端全仓 | 3529通过，0失败/忽略/过滤 |
| text-reference | 64通过 |
| fmt、Clippy、迁移与证据运输检查 | 通过 |
| 实际依赖图 | 15crate、38内部依赖边通过 |
| 独立消费者O0/O2 | 122原生执行，含90allocator运行、38真实分配失败；均匹配，live=0 |

作者/独审/全仓重放不叠加。root另核公共helper可零次或一次调用FnOnce，4次O0/O2执行通过；这是消费者122次之外的独立近邻。

为未用泛型定义检查Enum真实资格，一个具名旧源的Expand预算649→665；Types610保持。默认限额未增，其余具名旧控制保持，详见changes/246-250-owned-callback.md及246预算收据。本轮不作Rust编译速度、运行或RSS胜出结论。

本片仍不开放裸F返回、借入F的新公共拥有签名、State/容器/opaque工厂的新签名资格、长期外部回调、Type/Syntax或异步。通用callable helper内的显式defer仍守原237边界。下一主目标为同实例F→F转交，之后接Type/Syntax与外部长期生命周期；完整ZIL/ZUI和产品条件未完成。

CP维护独立：13远端研究枝与1本地旧枝退役，原历史已由远端归档标签独立恢复核验；CP25保留，未把研究成果冒充主线采用。环境GitHub读写、离线依赖和LLVM23原生链已核，见evidence/maintenance/。
