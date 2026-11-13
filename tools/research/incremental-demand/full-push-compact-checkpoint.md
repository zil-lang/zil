# Scalar SSA producer：44d完整三臂续接

当前实测 source/tool `44d30f7173e5778dae85fd7258e0eb06e65cf912`，compiler `634d4e96bf8ac37a1235fcb3275402aa81566f025f6f9ab795394c21a6e91b66`；main9170/compiler8d8保持原pin。本窗仅归属44d；后续 frontend owned Normalize 与 metadata prefix 不混入结果。精确pin、收据和摘要见[JSON](full-push-compact-checkpoint.json)。

每格仅2个 measured target_request，按 block 匹配计算 paired GM；下表时间为中位数ms，GM为 candidate/main、candidate/Rust（小于1表示本窗候选较短）。parent cold 不混入 cold。

|模式|请求|main|candidate|Rust|GM c/main|GM c/Rust|
|---|---|---:|---:|---:|---:|---:|
|check|cold|1298.78|913.88|1161.14|0.657|0.740|
|check|unchanged|95.92|3.25|15.85|0.034|0.205|
|check|private|605.69|76.20|474.58|0.119|0.162|
|check|multi|604.95|83.04|214.23|0.137|0.390|
|check|broad|866.76|1000.25|1135.20|1.149|0.877|
|check|positions|1072.62|438.67|920.15|0.411|0.477|
|check|error|373.56|45.57|258.23|0.122|0.178|
|check|recovery|552.51|59.03|230.68|0.107|0.256|
|check|restore|82.53|22.39|279.60|0.271|0.080|
|o2|cold|6347.40|5886.28|6669.48|0.929|0.882|
|o2|unchanged|520.00|33.20|30.53|0.063|1.187|
|o2|private|1540.95|278.18|566.45|0.181|0.494|
|o2|multi|1592.02|442.09|1028.06|0.278|0.430|
|o2|broad|5244.71|5296.49|5750.80|1.010|0.920|
|o2|positions|1347.67|729.37|1097.51|0.534|0.657|
|o2|error|401.18|57.60|254.91|0.144|0.228|
|o2|recovery|1747.99|282.34|547.47|0.163|0.516|
|o2|restore|725.09|64.21|440.21|0.090|0.146|

两模式各102 rows=54 measured+48 parents，两个controller退出0；原9场景与父链完整。O2共1080次native，90份完整ELF行、30对main/candidate完整ELF字节硬等、30对exports与实际CGU质量均过；真实CGU各16，分区内容仍分别检查。原始LLVM差异是受审表示变化的观测，未豁免whole ELF或native oracle。

native cold本窗5886/6347/6669ms、GM c/Rust0.882；private278/Rust566。但native unchanged33.20/Rust30.53、GM1.187为负；broad5296/main5245、GM c/main1.010，check broad1000/main867也为负。check cold914/Rust1161、GM0.740是本窗观测。仅两block，不宣称显著、稳定或全面领先，不跨窗124/554归因。

原425、全9请求及parents、完整优化/预算/独立Verify、CGU16/O2/jobs1/cacheall/cap256保持。ZIL resident请求计时与Rust1.99.1 Cargo/rustc新进程边界不同；ZIL LLVM23.1.4两DSO与Rust嵌入LLVM23.1.1不同。artifact/hash/native执行在计时外，候选首profile准入/hash计入cold；OS缓存未清空。成功输出保留，错误旧exe不执行；本窗未额外做output mutation。

native85/fmt在0d80通过；首backend编译E0614失败保留。backend269/driver211/Source9/retained1在4337通过，Clippy range-loop风格失败保留；44d严格Clippy/release通过。Astra完整4337审查+44d等价风格增量分别有收据，未虚报行为套件在44d重跑。Lower当前CGU报告与terminal retained-producer出处分别保存，终端复用不伪造16个当前hits。

124旧完整表固定于44d Git blob及V29 recovery `620e1fb3646892ed16c5c21fee924a965f9b52e8`；554/956旧证据仍由V28固定INDEX `090729ba21c163521b899ceab756984a3e389135`、recovery `534bd98fcdad9ad79d6ba8daa0e6a971729ea844`恢复。V30整体archive/recovery待封，未填未收到的fixed引用；无main merge，性能父目标仍开放。
