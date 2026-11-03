# 语言完整性来源样本

历史基线与实际观察见 `docs/validation/language-readiness-baseline.json`，核查日期2026-11-03。
当前本地恢复候选为 `a57a668b68044e37e870d058684f9364307e04b1`；本地验证完成不代表远端main合入或CI通过，完整语言尚未完成。
`controls` 保留既有源码到Verify正控；`pending` 是历史路径，不能仅按目录名判为当前缺口，也不注册为“必须拒绝”测试。

以下四份源保留原路径和字节，现已作为正控运行，不重复重写：

| 历史pending源 | 已本地验证的有限能力 |
| --- | --- |
| `pending/runtime-tuple-binding.zil` | 313普通有限Copy tuple绑定；321补泛型符号消费者。 |
| `pending/ct-type-generic-function.zil` | 314 Type/Syntax普通Copy泛型函数。 |
| `pending/ct-type-generic-record.zil` | 314具体纯记录CT组合；320补Struct Copy消费者。 |
| `pending/mutable-product-helper-gap.zil` | 315普通完整Product局部`&mut`同步helper。 |

317实际多模块组合消费者也已完成本地源码与O0/O2运行、错误/关闭清理验证，见`examples/mainline/recovery317`及`changes/317-recovery-consumer.md`。
316仅为`recovery/`中的25源能力核查，结果见`docs/validation/recovery-capabilities.json`，不是life/trait/异步或并发实现。一般life/trait、异步/Mutex及其余基础类型/模式仍是未完成范围。
大数组a18在当前工具链O0仍崩溃，生产修复未做；精简复现见`docs/validation/recovery-large-array.json`。

其余合法入口未来修复须保留原意并晋升正控。历史最小源的检查层级和317机器运行分别记账，不从未支持诊断推断安全，也不将源到Verify检查冒充原生执行或完整语言覆盖。

每份源可用 `zil check <path> --json` 重跑；判定以当前规范及其具名附件为准。
这些源覆盖典型缺口，不构成完整语言测试集；接口设计仍未固定的浮点/匿名union没有加入必修清单。

`design-probes/returned-captured-handler.zil`故意尝试当前仅静态入口的闭包转换，
应被现行规则拒绝；它只定位“返回后外部持有捕获环境”的需求缺口，
不能通过放宽`c_callback`来晋升。未来须提供真实稳定存储与关闭协议。
回调scope、返回静态guard、已有Store独占helper及普通局部Product对照
分别保留，防止把已支持能力重新列为缺失。
