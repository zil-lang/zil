# 0144 效果摘要阶段删除无消费者工作

141 默认预算的大循环图在 Analyze 报告 ResourceLimit；这不是 unsafe，也不是慢样本成功。检查当前实现发现 analyze_function(check=false) 每次重算描述 liveness，并在局部固定点完成后重复执行相同 transfer。这两项没有效果摘要消费者。

改动：仅 check=true 构建 afters；check=false 的 worklist 固定点完成即返回已累计效果。check=true 的最终逐块责任验证保持，独立 Verify 零改动，默认 Limits 零改动。旧完整 per-function 算法留在 cfg(test)，现有全部三节点有向图与边置换矩阵逐函数比较精确效果/诊断。

证明边界：Flow::after 所有读取都被 check 守卫；固定 effects 的 Flow 对输入状态确定；每次输入 join 改变均入队，空队列说明每个最终输入均已被执行；Effect 使用集合并增长。因此重复执行不增加新效果。可恢复错误仍由这些执行和最终 check=true 产生，不跳过活跃借片或清理核验。

预算接受性会改变：不执行的工作不再收费，所以原 ResourceLimit 可能变成成功；保持固定上限但不能声称预算诊断字节完全相同。不得将这种变化称为放宽安全判定。正式产品门/独审未执行。
