# 大CFG支配查询的区间索引

基线c6a60d3ec。stage5大模块诊断在8192函数/巨大聚合入口观察到当前机器preflight约322ms。源码进一步定位：Cooper式RPO已得到idom，但每个SSA use仍沿父链判断dominates；循环中复用entry alloca等会重复扫描长链。

保持原idom构造、reachable、不达块use规则、phi/首错顺序及所有机器验证不变；对>=32块的图一次迭代DFS标号dominator tree，祖先查询改为区间包含。小图保留原爬链，入口/自身在可达前提下直接成立。索引只由当前idom生成，不来自调用者或缓存，没有新的安全授权；不改任何逻辑预算/语言接受性。

写集沿已授权preflight.rs及本专项提案/证据。独立随机CFG全块对、循环/不可达/不可约图及深链差分旧walk；现有SSA/phi无效模型必须同样拒绝。随后按大模块完整CLI/原生行为和Rust对照裁决，不将局部查询复杂度直接称为全编译O(1)。
