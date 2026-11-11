# 暖命中请求的重复工作审计

审计对象：ZIL main [99035931d358add0eaed6d4535035c38413701dd](https://github.com/zil-lang/zil/commit/99035931d358add0eaed6d4535035c38413701dd)。本轮只读源码及恢复的历史报告；没有改生产、构建或计时。父任务给出的 lower约104ms、两规划扫描约63ms、整图key约46ms是已有测量输入，以下不把这些时间重新归因到未单测的子函数。

## 最强小候选：调度排序只计算一次 CGU cost

建议仅考虑 `build.rs:636` 的 `schedule.sort_by_key` 改为 `sort_by_cached_key`；保持闭包和排序键原样。这是一个单行的局部冗余消除，不改变机器模型、边、owner、验证、缓存身份或对象生成。

实际调用链是 `build → compile_units → schedule排序 → worker取unit → compile_api_unit → cache.lookup → 当前unit完整preflight`。所以对象全部暖命中时，这个排序仍执行，而且发生在命中查询之前。[build.rs:630–698](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/build.rs#L630-L698)

当前key闭包并非廉价字段访问：它遍历该CGU全部items，并调用每个Function.cost；cost又遍历所有blocks、累加instruction Vec的长度。它不遍历每条instruction内容，但每次比较会重新读取整个CGU的function/block结构。[model.rs:620–624](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-llvm-api/src/model.rs#L620-L624)

设CGU数为U，每个CGU取cost的结构扫描量为C_i；当前排序在比较中重复提取key，最坏是O(U log U)次key调用；预缓存key后提取成本为O(ΣC_i)，另加O(U log U)的廉价元组比较。Rust官方契约明确cached版本每元素最多调用一次key，同时说明简单key可能反而不划算；本处key有实际模型遍历。[Rust slice官方文档](https://doc.rust-lang.org/stable/std/primitive.slice.html#method.sort_by_cached_key)

### 为什么排序和产物不变

排序键完整保留 `(Reverse(total_cost), id)`，id在schedule中唯一，因而每个CGU之间都有唯一顺序，没有相等key的稳定性歧义。key只读模型，不含副作用或预算消耗；cached提取次序不会影响内容。后续worker仍按相同schedule分配，结果仍由id重排，错误/结果收集规则不变。模型和PreparedCache.machine_key都不改，现有验证仍在原位置执行。

### 收益上限与风险

这确定减少重复的cost提取，**没有证据表明它解释63ms或46ms中的大头**。U当前为8、上限64；cost只读取blocks/instruction长度，可能仅有很小完整请求收益。cached排序还增加O(U)临时key存储；小unit或单unit可能无净收益。它的价值是写集极小、输出和key可作精确等价检查，不能在未测前宣布显著提速。

也可显式把schedule装饰为(cost,id)再排序，但增加代码和维护面；优先用标准库已有的key缓存接口，不新加工作量启发式，不改调度政策。

## 其他实际路径：哪些不是本轮可删冗余

| 位置 | 已核事实 | 本轮判断 |
|---|---|---|
| partition_view_cached | ResourceHelpers先prepare，成功直接replay；纯多文件普通函数fixture的closures为空，在首guard即退回，之后才检查普通layout cache | 不能把该fixture说成两套完整planner先后执行 |
| layout action_key vs object machine_key | 前者绑定入口、分区选项、LIR观察字段及原始完整machine；后者绑定object namespace与实际分区/改名后unit | 两者是不同哈希流，不能复用digest或替换成诊断fingerprint；也不能漏掉实际imports字节 |
| layout缓存命中 | load仍做decode/valid，valid检查owner、序列、唯一归属、prototype合法性、源码身份及复制cost；随后valid_declaration_prelude独立检查被过滤声明 | 这些不是缓存命中的验证许可，不建议删除或用旧收据代替当前检查 |
| Layout.replay | remaining计数使最后消费者move原item，仅额外消费者clone；imports还需私有命名和direct-call重写 | 最后消费者move已存在；shared/Arc body会跨rewrite/fold/preflight/hash边界，非小改 |
| machine.to_text | API请求仍生成完整诊断sidecar，随后写出并由报告暴露 | 当前确实有渲染成本，但不是无人使用的中间字符串；直接renderer已在main，不能把删sidecar当同请求合同优化 |
| refs/direct_calls | 普通planner分别读全部符号引用和直接调用；现有Op.values与Val.symbols覆盖嵌套Array/Gep等引用 | 合并扫描可减少一次指令enum访问，但集合分配/排序与借用设计会改多个接口，尚未证明比当前快，不作为最强候选 |
| replay prototypes BTreeMap | 多次按item index查询确有log因子 | 改稠密表会增加按全图大小初始化/内存；只是换容器，不列为确定收益候选 |

源码证据：

- [partition_view_cached / 回退](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/machine_cgu.rs#L166-L258)
- [ResourceHelpers首guard](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/machine_cgu/resource_helpers.rs#L44-L65)
- [Layout.valid与action_key](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/machine_cgu/layout.rs#L179-L307)、[key构造](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/machine_cgu/layout.rs#L411-L497)
- [Layout.replay](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/machine_cgu/layout.rs#L40-L115)、[distribute_item](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/machine_cgu.rs#L602-L616)
- [PreparedCache.lookup / machine_key](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/object_cache.rs#L233-L323)
- [必需sidecar与实际unit导出](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/build.rs#L303-L308)、[写出与编译](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/build.rs#L450-L529)

## 旧实验排重与不继续扩查的边界

已查恢复目录previous-evidence中的review、diagnosis、patch及本轮research/zil-next-audit；没有找到schedule key缓存或sort_by_cached_key候选记录。该结论仅覆盖当前恢复文件，不声称查尽所有历史源码。

已存在的sha2硬件后端、BufferedHasher、直接文本renderer、checked-add必要条件与scalar无call短路，不再提议；sink、builder position、worker-owned unit析构以及之前否定的placement/import/key投影方向均不重试。

结论：可把“排序时重复遍历模型”作为一个极小且精确同IR/同key的候选；除此以外，本次未找到足够证据支持把主要暖命中扫描安全删掉，也未找到应宣称为全请求主要瓶颈的新增超线性算法。后续应先由root决定这一级别的预计收益是否值得正式窗口，不能因为候选简单就默认开展新一轮大矩阵。
