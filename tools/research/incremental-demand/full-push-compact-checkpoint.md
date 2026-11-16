# Full-push 精简恢复入口

唯一施工分支research/full-push-native；最新正式check源码 `54a0258ab9e081085254084e4b081309dc06eda9`（生产b530，54a只改断言）。

[当前重启口径与函数优先恢复](check-restart-scope-chain-20261116/summary.md)：check108请求/36格过；八私改0.814/R3.430s，用户边界峰13.641/R58.348MiB。Fresh重启0.657/R.043s，但Rust没启动；另同字节/缓存、仅mtime强制rustc对照1.152/R.326s仍输。check是dev opt0，native才O2；均CGU16/jobs1。

函数优先恢复省去整份Typed/Plan预解码/重基，保留manifest-only包定位和原当前检查/预算/Verify。driver232及最终多包无重写1/native发布2/strict/fmt/release过，未变收据按精确源复用。没有待完成构建或性能进程。

最后正式原生仅属7e08aeefa：冷3.759/R6.327s，关闭→先改→新进程首编3.385/R1.099s仍输，不转记当前源码。下一短板是重启全量Expand/Types/缓存恢复与发布、最新稀疏函数项持久化，以及对象命中前整机/分区重建。全面/main/工业门开放。

按[compiler-performance skill](../../../.agents/skills/compiler-performance/SKILL.md)，只保精简结论，不全量打包或LLVM重建。SDK18/native-link复用固定e85/V32；原Verify/预算/O2及研究默认关保持。175/98fa/021997数字各归原源，goal/archive不合main。
