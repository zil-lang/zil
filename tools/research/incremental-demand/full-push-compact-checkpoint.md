# 大项目编译痛点：当前已验收接续

实际编译源码 **1de00f63f80cf4534ec995dc51e106005b270ddb**，binary `4d040bb3dea0d3ca8b4ec06e38d47c359289a74c54ab3069b7bc90fd9f0fa5f7`；研究默认关闭，main仍9170。优先绝对节省秒数×真实请求频次、默认容量及用户磁盘增长，毫秒倍率不作主要出口。

原始5105声明、102文件、843875bytes全部原字节哈希未变，默认预算下cold/warm检查及O2原生全部成功，四次均当前独立Verify1；候选与Rust各24次原生oracle全部通过。main同输入四次Expand限额，不能作速度比。该容量链经独立运行收据复核接受。

[本轮绝对耗时与磁盘报告](current-large-project-reviews-20261114/large-project-current-weighted-coverage.md)分别列1de研究与30c stock，表中仅actual main/一个候选/Rust。九窗7完整通过、2千函数原生full-ELF首败，864行；失败保留comparisons空。

研究2500的17请求check合计main2.811s/候选1.642/Rust4.035；O2为11.436/8.572/7.833s，原生仍未胜Rust。1000的32私改检查，完整41请求为7.001/2.491/5.146s；真实频次不冒充工业使用频率。

2500 O2用户产物边界物理峰值main26.91/候选16.37/Rust42.51MiB；1000检查为53.74/13.62/12.16MiB，32次私改增长46.62/8.64/3.09MiB。只算明确用户缓存/产物、全局inode去重，非瞬时峰值，不含SDK/编译器宿主target/证据。磁盘仍有负场景。

stock30c整库6122/0/0、参考73、strict/fmt/release过；受控O2 17请求31.341s/main32.310/Rust25.800，暖路径回退及千函数full-ELF失败使main继续Hold。未把goal、归档或研究开关送main。

[完整恢复入口](evidence/current-large-project-20261114/RESTORE.json)：50份1,256,214,534bytes，38,370原始文件/5,872,518,806bytes，已全新目录恢复逐文件校验。包括本轮全部首败、新release/二进制、Source5105与九窗。旧八窗44份的RESTORE固定在1de提交；98fa正式数字仍固定59fd，绝不归给021997或之后源码。

本轮生产实现为原Source重复遍历融合与实际LIR成本的可选native捕获；raw旧费域与新实际费域各保N/N−1、fresh/warm/失败恢复，临时峰1280不放宽。257 Source与207未变driver成功复用；5517修复旧6及新1，实际9项含2重叠，组成driver214，非整库重跑冒认；Source9/发布1实际过，1de strict/fmt/release实过。更早API86/backend273按精确未改源码复用，完整独审和收据见JSON。

仍是合成秒级项目，工业大型workspace、复杂泛型、宏、async覆盖不足，不宣称全面超Rust或解决分钟级痛点。下一优先：2500 nativecold2.821s/Rust1.390s的绝对差、连续编辑磁盘增长、真实机器差异质量门，以及受支持的大型真实软件场景。
