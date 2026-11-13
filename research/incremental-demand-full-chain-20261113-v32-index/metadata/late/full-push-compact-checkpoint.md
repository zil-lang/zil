# Cross-CGU cohort：98fa原425完整三臂结果

正式 compiler source `98fa664743c4a2d94b5dc1eba5b61c7c1867ca4c` / binary `d905664388e30eb3aef415af43fe1ac4c00d5469c537f03308efcb179b29ccef`；measurement tools `0db08422dde4de3c222b2c72c444c9f375476612`。实际main9170/compiler8d8，Rust1.99.1。精确收据和完整摘要见[JSON](full-push-compact-checkpoint.json)。

每格仅2个 measured target_request，按同block匹配；时间中位数ms，GM为candidate/main、candidate/Rust（小于1表示本窗候选较短）。parent cold不混入cold。

|模式|请求|main|candidate|Rust|GM c/main|GM c/Rust|
|---|---|---:|---:|---:|---:|---:|
|check|cold|1012.04|1150.15|1120.83|1.190|1.041|
|check|unchanged|89.13|3.10|16.22|0.035|0.195|
|check|private|617.77|51.07|211.73|0.084|0.243|
|check|multi|663.49|104.88|245.02|0.156|0.420|
|check|broad|843.21|837.32|931.11|0.983|0.898|
|check|positions|931.41|476.68|907.25|0.513|0.525|
|check|error|361.32|44.78|255.69|0.124|0.179|
|check|recovery|657.39|69.47|220.37|0.106|0.326|
|check|restore|82.21|25.57|185.73|0.311|0.138|
|o2|cold|6068.70|5900.64|6610.61|0.975|0.893|
|o2|unchanged|696.98|102.34|18.28|0.067|2.508|
|o2|private|1653.88|486.95|668.46|0.283|0.702|
|o2|multi|1624.87|496.60|982.43|0.305|0.504|
|o2|broad|5014.44|4611.26|5557.02|0.919|0.830|
|o2|positions|1282.21|740.43|1020.72|0.565|0.708|
|o2|error|361.61|42.05|182.84|0.116|0.230|
|o2|recovery|1446.24|325.44|448.27|0.224|0.722|
|o2|restore|506.66|74.15|575.03|0.147|0.132|

02两窗各102valid rows=54 measured+48 parents，双controller0；各34组质量收据通过，O2有1080native、30对完整ELF字节硬等/exports一致，三臂实际CGU16并检查真实内容。允许受审IR表示变化，未减原优化、预算、独立Verify、model preflight或LLVM前后Verify。

冷路径每block真实cohort观察：IR stage重放494次、MIR重放237、MC observer重放237函数并跳过78962条MI；fallback/disabled均0。IR494是阶段事件数，不是494个独立函数。broad同IR/MIR，MC跳过80258。private/multi/recovery的新cohort重放为0，原prior的MC总重放分别39/58/39，不混称cohort收益；缺观测保留null。

负值必须保留：check cold候选1150/Rust1121ms，GM1.0406；native noop候选10.51/194.17ms、GM c/Rust2.508，慢样本resident CPU约0.18s，原因未归因。native cold本窗5901/main6069/Rust6611、GM0.975/0.893；仅两block，不宣称显著、稳定或全面领先，不跨536/44d窗口归因。

d662实际通过native86/backend269/driver211/Source9/retained1/严格Clippy/fmt；98fa仅观测timer scope修正及说明字符串，经非作者源码等价审查复用原门，release新建通过，不冒认行为套件重跑。0db是测量工具pin；分析工具随后修schema，其文件SHA与成功analysis-02 SHA分别固定。

01窗check0、O2因IR child timer超过parent严格失败；98fa修正计时归属，未放宽child<=parent门。02首次分析KeyError:archive源于symbols记录实际为path+sha256；首败保留，修工具后的唯一analysis-02有效。此次仅复核记录和文件元数据，未重哈希大blob或重跑原生。

CGU16/O2/jobs1/cacheall/cap256与原9请求/parents不变；ZIL resident与Rust Cargo/rustc新进程、ZIL LLVM23.1.4与Rust LLVM23.1.1的边界差别保留。artifact/hash/native执行在计时外，首profile准入计入cold，OS缓存未清空，成功输出保留。

全局点对点push仍未完成：中央ReactiveGraph仍只评估BodyKey，下游真实稀疏消费者不等于完整中央传播。当前完整World/context投影、model preflight、身份/分区/对象键及全局聚合仍有成本；cohort身份不能授权跳过当前完整检查。无main merge，研究默认关闭，性能父门仍开放。

536旧完整表及cold负值由0db固定Git blob和V31 recovery `8ee98e6820158865ede7d68dd63e069f62ca076f`保全。当前V32整体archive/recovery待封，不填未收到的fixed引用。
