# Production chain：b6完整六block结果

Compiled `b6e019894986b43a1bf1a81cf549a0850d66b9b4` / tree `be65cb91ddcf01f9c767ab8e502b6382b4f133e3` / binary `a1e0e50ceb6f880805e124dc8a5486ebdc0bf82c6d80ffb9e2df8490b5053c67`。测量tools `37fb410dd00e1f92b6ec660f313f8cf57d84fd71`；后处理 `c59a3878654efe589e5d9da723d0a17d2411b29c`。测试修复37fb没有重标b6 release，也不混用98fa/021997性能数字。

完整Source→scalar生产→Types/Analyze/独立Verify→backend→IR/MIR/MC→原生发布已接通：位置编辑按声明原字节/lexer gaps/属性环境及当前完整源重绑定；表示谓词和线程需求按强owner更新；原生产品完整匹配后共享、解析借用拥有的不可变payload。默认预算、完整O2和当前检查保留。

At b6 native86/backend271/Source9/retained1/strictClippy/fmt/release passed. Expand115+1failed and driver210+1failed preserved. At37 test-only repair actual Expand116, targeted driver-history1 (210filtered), Clippy/fmt passed; other210 driver cases source-equivalent, not claimed rerun.

Astra按精确生产SHA审查接受；37测试等价复核接受。分析器继承的总量/逐block缺口曾被拒绝，c59修复后独审接受，65个实际metadata正负控通过，未重测原时序。首次失败与全部报告保留。

每格6个measured target_request，父请求不混入。实际main9170/candidate/Rust、original425、CGU16/O2/jobs1/cacheall/cap256。时间中位数ms，GM按同block成对（<1候选较短）。

|模式|请求|main|candidate|Rust|GM c/main|GM c/Rust|
|---|---|---:|---:|---:|---:|---:|
|check|cold|861.78|513.36|1040.90|0.670|0.559|
|check|unchanged|88.00|3.14|11.58|0.031|0.266|
|check|private|581.09|67.73|183.16|0.121|0.351|
|check|multi|470.60|81.03|180.86|0.211|0.520|
|check|broad|746.14|785.26|857.39|1.006|0.840|
|check|positions|908.05|425.52|722.92|0.481|0.564|
|check|error|359.38|51.40|214.85|0.136|0.256|
|check|recovery|490.80|59.58|177.68|0.116|0.303|
|check|restore|82.70|23.94|170.95|0.293|0.137|
|o2|cold|5681.72|4486.45|6010.72|0.778|0.741|
|o2|unchanged|532.25|8.68|14.06|0.018|0.711|
|o2|private|1522.03|343.22|563.93|0.225|0.634|
|o2|multi|1598.02|464.00|925.49|0.291|0.513|
|o2|broad|5277.92|4453.49|5449.96|0.837|0.812|
|o2|positions|1338.01|576.32|966.42|0.374|0.541|
|o2|error|380.23|63.44|187.37|0.152|0.309|
|o2|recovery|1482.16|381.44|415.43|0.260|0.889|
|o2|restore|477.12|62.65|415.97|0.136|0.161|

两窗各306valid rows=162measured+144parents，controller0。native3240次oracle、90对完整ELF/exports一致；226个native artifact blob全哈希复核。逐block各51/27/24/17/540/15、每成功row12native及完整parent/三臂/pair位置都已核，不只检查总量。

本窗18个中位数及配对GM均短于Rust，不能宣称所有样本或所有程序稳定全面领先。负结果：check broad中位数785.26/main746.14，GM c/main1.006；O2 unchanged候选六样本20.834/11.997/5.145/5.359/4.931/38.372ms，其中block0/5慢于Rust。check multi/broad的block3、O2 recovery的block0/3也慢于Rust；原值、CPU/RSS与未归因边界完整保留。

真实工作观测：位置编辑22模块Normalize/Restore/Lower执行0，reusedLower22且当前SourceMap验证22；仍有425份syntax/Core位置副本和SourceMap复制，未声称零物化。原生无修改表示检查0/reused425、线程需求扫描0/reused425。原生仍执行当前原有授权链，统计不充当语言Verify证明。

ZIL resident send-to-response excludes startup; Rust new Cargo/rustc includes startup. Source writes/hash/artifacts/native executions outside compile timer; first profile admission in cold. OS caches not flushed; CPU/RSS scopes differ; no memory advantage inferred.
Rust1.99.1/LLVM23.1.1; ZIL LLVM23.1.4, original stock2bf vs SDK18 785 actual maps. Equal CGUs do not mean equal partitions or checking guarantees.

全局ReactiveGraph传播及更广程序/特性的全面稳定Rust领先父门仍开放。未合main，研究默认关闭。

完整原始证据由分片archive+manifest保留并新目录恢复验证；源码及完整证据已推送并核对远端 `9b0a8b6e29c49673c234cca5704430d5e4455e0e`，checkpoint blob 已读回一致，见 [publication receipt](full-push-publication-receipt.json)；归档内认证待办描述保留为历史状态。嵌入源码tar只排除上一轮非源码raw证据目录以免重复，完整Git树清单和delta bundle保留原提交。
