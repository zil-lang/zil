# 基础语言组合收口

当前阶段备份：`task/G10.6.3@ed8e3a5de6befb15b515f6b124e992792414a8ca`，已推送并核完整远端 SHA。main 仍为 `b07b85dd49b3df6f382734d509919a2a6013c97d`，本记录不是 main 验收。

| 范围 | 精确来源与实际结果 |
| --- | --- |
| 融合源码 | G20 `f0ee4f821`、G21 `7dd7bb0eb`、动态数组 `1fdd1b746`、callback `6a95603ed`、资源成本 `5cf970ba5`、backend `9f91ccc5e`；均已独立限定复核。此前已接受trait/lending、Mutex helper及普通大值片继续保留。 |
| 独立两核 | `b020d122349c1b83f89fb9143223a3678e1ca5a8` 上 Analyze 207/0、Verify 459/0；`out/runs/foundations-integration/assembled-kernels.log`。 |
| 数组合流 | `36a65dbc6aff4ed73d8d555e52122f8a29865a56` 上405/2；两失败仅尚未main的新动态预算pin。原分支未含已接受的balanced ancestry优化，实际新费用降低。`82ca680c958e118636a002f38be6ea450f913ff8`仅校准新pin，动态19/0；N−1/fresh/warm/增长与所有旧pin不变。两个日志及源码差已非作者复核。 |
| 拥有相关 | 同一融合生产上209/1，另明确过滤原完整程序两项；实际单失败也是原4095的Analyze-only用例，首阻点ByteBufferBorrow，非其它回归。日志`assembled-owners.log`保留。 |
| 后端真实执行 | 候选`9f91ccc5e`，原无callback诊断四模块及ErrorArm消费者共8次双后端O0/O2执行，Verify1；该近控不是原4095完整程序。 |
| 路径与声明 | 最终`ed8e3a5de`的全树delivery结构检查4024通过；wide Array两输入只迁名、字节不变，真实宽数组专项1/0。 |

原4095四模块Source保留逐字，fingerprint `d9afb8c2667ac61471aa1f37bd2fb9277abcbab5df37090bf4f8f6b8d32ec676`。G22阶段实现已越过实际字节视图Unsupported，当前真实Analyze默认预算耗尽，尚未Verify或48次原生执行；不调高Limits、不用近控替换原消费者。最终共同门、冷恢复和main集成尚未完成。

G9自动增长Map/Set仍被callable普通可变参数非局部Commit效应合同阻挡，未验收。大型普通Direct拥有值的ABI成功不覆盖普通Closure，后者另派G10.3.6。
