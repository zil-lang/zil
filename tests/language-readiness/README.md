# 语言完整性来源样本

当前共同候选B `14832d5a35630b7002c79e4df47b0cbb007570b6`的增量结果在
`https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/language-gaps-334-final.json`：同52输入当前27/13/12，下面的
52/18收据仍属于历史40fb。新的组合component另四配置，原两CT源另八次native；
不将旧CLI结果归给B。记录变体、有限default、Copy-T/life和具体父闭包线程已
有限贯通，完整Option/trait/life/dyn/异步/库与产品门仍未完成。

当前十方面核查见 `validation/results/language-readiness/language-gaps-baseline.json`，基线为已合入main的
`40fbcca6ef506f3cb8e1d40a4f8995c788627da0`：52源检查和9源的18次API O0/O2
原生构建/执行分别记账。复跑入口为 `current334/README.md`。历史基线与实际观察
仍见 `https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/language-readiness-baseline.json`。
上一批本地受测候选为 `a57a668b68044e37e870d058684f9364307e04b1`，后续结果元数据经PR28合入远端main `0d7d74f7`；CI因账号支付/额度未启动，不能称为CI通过。322–328的有限life/trait/Direct线程与原a18修复已进入本基线，完整语言尚未完成。
`controls` 保留既有源码到Verify正控；`pending` 是历史路径，不能仅按目录名判为当前缺口，也不注册为“必须拒绝”测试。

以下四份源保留原路径和字节，现已作为正控运行，不重复重写：

| 历史pending源 | 已本地验证的有限能力 |
| --- | --- |
| `pending/runtime-tuple-binding.zil` | 313普通有限Copy tuple绑定；321补泛型符号消费者。 |
| `pending/ct-type-generic-function.zil` | 314 Type/Syntax普通Copy泛型函数。 |
| `pending/ct-type-generic-record.zil` | 314具体纯记录CT组合；320补Struct Copy消费者。 |
| `pending/mutable-product-helper-gap.zil` | 315普通完整Product局部`&mut`同步helper。 |

317实际多模块组合消费者也已完成本地源码与O0/O2运行、错误/关闭清理验证，见`examples/mainline/recovery317`及`changes/317-recovery-consumer.md`。
316仅为`recovery/`中的25源历史能力核查，结果见`validation/results/language-readiness/recovery-capabilities.json`。
334复跑相同字节时，life参数返回和具体泛型trait已成功，三份未声明life源已明确
拒绝。有限life-only、具体泛型trait和Direct线程不代表一般life/trait、异步/Mutex
或其余基础类型/模式已经完成。`current334/partial-copy-tuple.zil`保留已支持的
Copy tuple通配部分解构对照；新增usize/u64及类型/life混合源保留合法目标。
大数组原a18已由324有限生产修复，两后端O0/O2限定机器复验通过；旧崩溃报告原样保留。一般巨型按值ABI尚未完成，具体边界见`codegen-pending/README.md`。

其余合法入口未来修复须保留原意并晋升正控。历史最小源的检查层级和317机器运行分别记账，不从未支持诊断推断安全，也不将源到Verify检查冒充原生执行或完整语言覆盖。

每份源可用 `zil check <path> --json` 重跑；判定以当前规范及其具名附件为准。
这些源覆盖典型缺口，不构成完整语言测试集；接口设计仍未固定的浮点/匿名union没有加入必修清单。

`design-probes/returned-captured-handler.zil`故意尝试当前仅静态入口的闭包转换，
应被现行规则拒绝；它只定位“返回后外部持有捕获环境”的需求缺口，
不能通过放宽`c_callback`来晋升。未来须提供真实稳定存储与关闭协议。
回调scope、返回静态guard、已有Store独占helper及普通局部Product对照
分别保留，防止把已支持能力重新列为缺失。
