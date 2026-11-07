# 语言支持与缺口输入

本目录的 Source 是有限能力正反例和合法目标探针。目录名 `pending`、`recovery`、`codegen-pending` 是冻结来源名，不能单凭名字判定当前支持或永久拒绝。每份源可用 `zil check <path> --json` 在当前编译器重跑，源码到 Verify 的检查、原生执行和完整语言验收须分别记账。现行语义以 `_zil_design.md` 与具名附件为准。

[冻结语言缺口收据](../../validation/inputs/language-readiness/language-gaps-baseline.json)绑定旧主线 `40fbcca6ef506f3cb8e1d40a4f8995c788627da0`，包含 52 份 Source 检查及 9 源×O0/O2 的 18 次机器运行。后续候选 `14832d5a35630b7002c79e4df47b0cbb007570b6` 的同源投影、组合/CT 机器检查见[精确历史结果](https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/language-gaps-334-final.json)。两套结果不可跨源码累加；当前主线须重新运行并绑定其二进制、工具链和输入。复跑入口见 [冻结基线回放](language-gap-replay/README.md) 和 [恢复探针](recovery/README.md)。

下列原 `pending` 输入已作为有限正控，仍保留原路径和字节：

| 输入 | 覆盖行为 |
| --- | --- |
| `pending/runtime-tuple-binding.zil` | 普通有限 Copy tuple 绑定及泛型符号消费者 |
| `pending/ct-type-generic-function.zil` | Type/Syntax 普通 Copy 泛型函数 |
| `pending/ct-type-generic-record.zil` | 具体纯记录 CT 与 Struct Copy 消费者 |
| `pending/mutable-product-helper-gap.zil` | 普通完整 Product 局部 `&mut` 同步 helper |

`controls` 是源码到 Verify 的正控。`language-gap-replay/partial-copy-tuple.zil` 保留 Copy tuple 通配部分解构；新增 usize/u64、类型/life 混合源是合法目标，不将旧 Unsupported/SafeReject 变成规范禁令。返回捕获环境的 `design-probes/returned-captured-handler.zil` 定位真实稳定存储与关闭协议的需求，不能仅放宽 `c_callback`；回调 scope、guard、Store 独占 helper 和局部 Product 是相邻对照。

大型数组的限定修复与未覆盖 ABI 见 [大值输入](codegen-pending/README.md)。多模块组合消费者的完整返回值、清理和机器运行按其实际收据解释；不从最小 Source 检查推断这些行为。更广 Option、trait、life、dyn、异步、库和产品目标需独立验证。
