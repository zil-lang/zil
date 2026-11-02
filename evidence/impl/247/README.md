# 247 作者限定交付

生产代码固定 `b9818e77d`；完整身份、命令和输入摘要见 [RESULT.json](RESULT.json)。本片只写 Analyze、impl247 测试及本目录，没有公共 IR/schema 变更。

最终限定执行 **26 tests，26 passed，0 failed/ignored**。源码测试7项、资源责任测试14项、Store profile2项、capture边界1项、旧预算各1项；历史重跑不累加。Rust assertion 未逐次埋点，不以测试数量代替断言计数。

原三个红源保留原字节并真正到达 Types→Analyze→Wire30→Analyze；压缩 TypedIR、候选及原Wire在 stages/，摘要在 final/stage-artifacts.json。总计13个不同源码正例、5个源码Analyze拒绝和4个明确标记的源码派生IR破坏。原借用复活误收与 Clippy 初次失败保留；最终运行均通过。

拥有参数逐次转交，结果为 Incoming；helper roles/traces与实际可写环境独立检查。新签名没有扩大 capture、State/array/Store callback 元素或 opaque factory 资格。实际地址根及其last-use沿相关调用图重建，结果同型不能复活已转交输入的窗口。

新原源 Analyze N=13556；raw FnMut两次调用N=4525。均验证 N/N−1/fresh。旧flat Product Analyze8673、旧Store捕获Analyze7129不变；默认限额未改。

本片没有新profile Verify、原生分配失败或全量结果；分别由248、249/250和root验收。Rust四目标和突破判断见RESULT.json：有限安全与表达能力有证，没有同语义Rust运行/RSS、维护成本或编译速度比较，也没有产品合取结论。
