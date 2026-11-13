# Owned frontend / metadata：536完整三臂续接

当前实测 source/tool `536cf228829064a3b78ac71497effa2a03283cf6`，compiler `4f01758cb6183a7723425c68556d2d03dad32b611046d4dd76afa31ab3dd7ccc`；main9170/compiler8d8保持原pin。后继 C++/Rust body-cohort 施工不属于536。精确pin、收据、计数和摘要见[JSON](full-push-compact-checkpoint.json)。

每格仅2个 measured target_request，按同 block 配对算GM；时间中位数ms，GM为 candidate/main、candidate/Rust（小于1表示本窗候选较短）。parent cold不混入cold。

|模式|请求|main|candidate|Rust|GM c/main|GM c/Rust|
|---|---|---:|---:|---:|---:|---:|
|check|cold|1427.28|931.79|1328.95|0.625|0.670|
|check|unchanged|96.82|3.52|20.13|0.036|0.176|
|check|private|613.44|79.06|500.91|0.127|0.185|
|check|multi|641.92|90.79|356.65|0.141|0.256|
|check|broad|983.26|793.82|1021.56|0.804|0.776|
|check|positions|1145.86|491.69|777.41|0.433|0.627|
|check|error|655.28|49.47|238.39|0.076|0.208|
|check|recovery|645.81|61.59|297.10|0.096|0.217|
|check|restore|101.30|25.43|284.48|0.251|0.090|
|o2|cold|6136.94|6700.53|6300.53|1.092|1.063|
|o2|unchanged|570.62|9.25|54.35|0.016|0.251|
|o2|private|1498.31|270.49|797.83|0.181|0.342|
|o2|multi|1687.31|462.87|911.06|0.273|0.507|
|o2|broad|5371.27|5210.33|5760.76|0.975|0.904|
|o2|positions|1482.67|473.43|1045.25|0.318|0.451|
|o2|error|323.24|43.32|219.16|0.134|0.201|
|o2|recovery|1611.69|258.71|423.46|0.160|0.613|
|o2|restore|486.13|73.47|479.45|0.150|0.152|

两模式各102 valid rows=54 measured+48 parents，双controller0；原9场景及父链完整。O2共1080native、90完整ELF行、30对main/candidate完整ELF字节硬等、30对exports与真实CGU质量均过；各16个实际CGU，分区内容独立检查。允许受审IR表示变化，未放宽whole ELF/native强门。

native cold仍负：candidate两block6545.12/6855.93ms，Rust6435.78/6165.27；中位c6700.53/main6136.94/Rust6300.53，GM1.092/1.063。不能沿用44d的小幅冷领先。noop候选10.23/8.28，但Rust14.18/94.52有大波动；54.35中位数及GM0.251不是稳定下限或显著领先证据。两block不足以宣称全面/稳定领先。

实际 `frontend_stats.producer` 冷两block均 normalized深复制425、Restore深复制0、ownedRestore425、materializations0；已观测增量producer的materializations均0。unchanged/restore计数为空，不补0。metadata费用绑定保留当前owner/位置/target/limits并省去重复完整type扫描；本窗没有独立metadata节省耗时计数，不做跨窗归因。native冷resident RSS候选372584/372456KiB、main444596/444964KiB，仅为本窗比较。

原425、完整优化/预算/独立Verify/model preflight/LLVM前后Verify、CGU16/O2/jobs1/cacheall/cap256保持。ZIL resident与Rust1.99.1新进程计时边界、ZIL LLVM23.1.4与Rust LLVM23.1.1不同；artifact/hash/native执行在计时外，首profile准入计入cold，OS缓存未清空。成功输出保留、错误旧exe不执行；未额外mutation。

门归属：01 c403 Expand E0478初败；02 1c3 Expand111通过、backend测试CallableKind::Shared编译初败；03 eece backend269/driver211/Source9/retained1通过、Clippy两style失败；04 5a18 Expand111/Source9/retained1通过、剩余large-enum失败；05 536严格Clippy/release通过。原失败均保留，不虚报行为套件或native85在536重跑。

44d完整旧表固定于2a6257 Git blob及V30 recovery `c29d8028112539bbfda74b883cb53b80984fd571`；V31整体archive/recovery待封，未填未收到fixed引用。八阶段剩余边界见[源码审计草稿](/workspace/zil-push-main/out/drafts/point-to-point-current-536.json)；143/400仅body结构计数，非复用授权或新成果。无main merge，性能父目标仍开放。
