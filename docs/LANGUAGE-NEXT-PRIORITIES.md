# 当前语言主线优先级

此前已合入主线的PR27阶段（`8bbb38f2`）包括：拥有Event→具体闭包/公共dispatch→拥有Reply、F原值转交、Type/Syntax、同线程结构化外部回调、泛型内具体资源准备、普通具体闭包组合、共享Product参数捕获及泛型直接enum错误返回，并补齐泛型显式初始化、普通/具体闭包实参的错误联合唯一注入、独立泛型约束和实际回调返回组合。各有限范围见对应LANGUAGE文档，历史阶段文档的旧“下一目标”不覆盖这里。

本地恢复候选`a57a668b68044e37e870d058684f9364307e04b1`已完成313有限Copy tuple绑定/321泛型符号消费者、314 Type/Syntax普通Copy泛型函数与纯记录/320 Struct Copy消费者、315普通完整Product局部`&mut`同步helper、317实际组合消费者的本地验证。全量默认4055/0、reference3947/0，各422组且零忽略/过滤；fmt/clippy、text64/0、migration9/0通过。此批后续元数据已由PR28合入实际远端main `0d7d74f7`；CI因支付/额度未启动，仍无CI通过结论。本轮候选备份为`task/language-mainline-20261103`。316只是25源能力核查，不是新语言实现。

1. Option及剩余基础类型/模式、一般life/trait、关联类型和动态抽象按真实组件API接续。有限Copy tuple、普通Product局部可变helper、Type/Syntax普通Copy泛型组合已经本地验证，不把它们重新列为待实现。实际源码、成功对照及有限边界见[剩余清单](LANGUAGE-REMAINING.md)。
2. 324已修复原大数组a18，两后端O0/O2明确限定复验成功；继续独审/组合验收和一般巨型按值ABI缺口。旧崩溃记录保留于`docs/validation/recovery-large-array.json`，当前限定结果见`changes/324-large-aggregate-lowering.md`。每批语言增量继续贯通源、核验、机器结果及失败清理，避免只有语法准入。
3. 同线程结构化scope已经可以覆盖持续同步事件环；返回后保留捕获环境的外部登记、跨线程及异步生命周期按真实消费者的独立义务推进，不仅因运行时间长而要求异步。

`tests/language-readiness/pending/`中的`runtime-tuple-binding.zil`、`ct-type-generic-function.zil`、`ct-type-generic-record.zil`、`mutable-product-helper-gap.zil`四源保留原路径和字节，现已作为正控运行，不盲目重做。原`tests/generic-error-enum/pending/`两个源也保持原字节，已晋升正控；历史目录名不代表当前仍失败。首轮过严符号规则导致的两项回归及修复见[本轮结果](LANGUAGE-GENERIC-INJECTION.md)。276已清理CI强制原始产物归档，原测试门保留。不重启CP研究、测试目录大重组或Skill重写。一般life/trait、异步/并发、Mutex及剩余基础类型/模式仍未完成；完整1.4/ZUI、B0及产品指标尚未完成。
