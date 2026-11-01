# 相同显式项目输入的前端结果复用

基线240bb37d806be81f82360e626d74af1844e973fc（生产实现5cf1c417）。阶段2计量发现：512模块完全未变时，默认容量只留28个Parse，驱动仍补做484次Parse，再命中Expand/CT/Types。已有完整TypedProgram的成功查询身份包含所有源码/顺序/路径/模块/文件身份、全部逻辑额度、build inputs和目标；重新物化其已淘汰的上游raw值没有提供新的输入事实。

候选仅针对显式ProjectSources、stop_after≥Types、未请求中间IR或磁盘源码缓存的请求。在现有profile、源图/费用和产物路径保护之后，查找精确当前Types键；要求私有process-local raw TypedProgram以及完整可达依赖收据有效，检查Types→CT→Expand→按模块顺序Parse的原查询形状。缺失或不符即回原流水线，不从磁盘读取或制造新接纳证据。

按原顺序重放这些已完成raw查询的Reused事件和每事件前后取消检查，只touch仍存在的raw owner，保持容量与可达收据回收。相同完整预算下复用原成功前端计算，不变更额度定义或新增跨额度授权。原始TypedProgram随后进入原Analyze及本次独立Verify/Lower/Build；不缓存Verified、不按哈希跳过Verify。

源码/位置/顺序/路径/模块/根、预算、build inputs或target变化都必须匹配另一完整键；失败、不完整依赖、部分阶段及要求中间产物/源码缓存的请求保守走原路径。文件系统入口继续实际发现并读取来源，不借此绕过新增/删除/歧义/符号链接检查。取消、诊断与产物保护做fresh对照，observer的Executed→Reused正是复用工作量变化，不改事件顺序或公开schema。

先以真实源码、N/N−1、逐阶段取消、工作集淘汰、依赖收据破坏和raw反例证明边界，再重新独立审核与完整计量。旧四窗和已审报告保留原身份，此候选不是函数级语义增量，也不将显式内存项目收益冒称独立CLI收益。
