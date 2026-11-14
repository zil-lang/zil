# Final-value reuse：8ce584完整链结果

实际 compiler source `8ce584f81a405b8c5aa69e555b80ede276f07143` / tree `35a051da13de600c71ddc9ecb76edbe82cf23c85` / binary `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810`。本轮 tools 同源码；严格分析器单独绑定 SHA。b6 六block、021997、98fa 的结果保留原归属。

当前预检后共享精确不变的 Product/配置/分区与预检容器；同位置仅选候选，重排精确 fallback 和每 unit checker 保留。诊断改变清除旧文件资格；backend 暂存延后，driver 的暂存、当前文件复查、恢复和双文件事务保留。

本SHA实际 backend273、driver211、Source9、扩充 retained1、CLI及backend/driver all-target strictClippy、fmt、release全部通过。native86/Expand116按精确未改源码复用原收据，未冒认新跑。Astra非作者精确差异独审接受。

完整 original425，实际 main9170/候选/Rust，CGU16/O2/jobs1；check与native分开。每格2个 measured target_request，父请求不混入。时间中位数ms，GM同block成对，小于1候选较短。

|模式|请求|main|candidate|Rust|GM c/main|GM c/Rust|
|---|---|---:|---:|---:|---:|---:|
|check|cold|1038.07|716.31|1045.73|0.668|0.666|
|check|unchanged|88.30|2.87|13.64|0.032|0.211|
|check|private|524.96|66.51|204.62|0.126|0.321|
|check|multi|521.52|125.16|236.30|0.228|0.505|
|check|broad|791.09|562.17|802.98|0.716|0.689|
|check|positions|654.43|331.67|698.77|0.510|0.477|
|check|error|341.91|42.78|187.06|0.125|0.229|
|check|recovery|487.83|67.90|219.53|0.135|0.303|
|check|restore|88.90|24.55|186.00|0.275|0.132|
|o2|cold|5564.25|4438.28|6164.70|0.797|0.720|
|o2|unchanged|474.23|13.71|27.94|0.025|0.482|
|o2|private|1535.40|356.15|564.70|0.230|0.622|
|o2|multi|1543.02|448.07|909.50|0.290|0.493|
|o2|broad|4904.23|4182.51|5332.91|0.854|0.785|
|o2|positions|1130.32|570.83|860.44|0.488|0.655|
|o2|error|310.51|47.70|170.96|0.151|0.274|
|o2|recovery|1243.57|415.58|446.23|0.319|0.895|
|o2|restore|465.44|101.05|504.15|0.198|0.185|

本窗 18/18 格中位数及配对GM均短于Rust。仅两block，不据此宣称稳定或全面领先，也不跨b6六block窗口归因。负结果：O2 recovery 在 block [1] 慢于Rust，候选两样本 [289.810002, 541.342419]ms、Rust [390.457242, 501.995707]ms；原因未归因。另有 check/broad block1 候选663.060208ms、main620.657598ms，慢约6.83%，也完整补列。全部逐block样本及相对main/Rust的负观察见JSON的 negative_observations。

ZIL常驻与Rust新进程启动、LLVM版本与分区、OS缓存、CPU/RSS范围差别均保留；没有推断内存优势。全局传播、一般语言稳定超越Rust仍开放，研究默认关闭，无main merge。

完整原始记录、当前binary、独审报告和收据由分片archive保留并新目录恢复校验。SDK18/native-link沿用固定V32依赖，不重建LLVM；源码及证据只推 research/full-push-native。
