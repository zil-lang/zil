# 错误联合上下文的普通组合

本轮候选把同一唯一注入规则接到泛型显式初始化、普通函数实参和已知具体闭包实参。其它实参可提供独立泛型约束，实际求值保持源码顺序；真实F声明的返回类型可用于枚举载荷与普通实参。完整联合原值直接传递，名义不匹配、双侧歧义和默认整数不匹配仍拒绝。

未确定外层类型可能在未来实例与另一侧同型，定义检查现在要求证明另一侧确实不同，包含未调用泛型的返回位置。旧实例阶段仍会拒绝歧义，本轮修的是定义期一致性，不声称修复已证实的运行期内存漏洞。公开IR、ABI、默认额度及拥有/调用权限不变。

[精确提交和检查](validation/generic-injection.json)、[非作者泛型审核](validation/generic-injection-review.json)、[非作者闭包审核](validation/closure-error-injection-review.json)、[独立消费者](validation/generic-injection-consumer.json)分别记账。组合四个driver目标47/0、格式及三个相关crate全部目标Clippy通过；最后21行声明结果hint增量由作者限定复验及独立差异审核，最终全仓CI仍待完成。

新增泛型10源共40个原生场景，具体闭包5源共20个场景，均覆盖LLVM API/clang与O0/O2；原值/错误tag、前后实参副作用各一次、defer和FnOnce环境在错误出口的清理均有实际结果。复跑不增加覆盖数。Expand三新固定边界与五个历史边界单列，N−1应为ResourceLimit，不提高默认预算。

初始误拒、旧未调用模板漏审、阶段性Clippy失败及首次API feature配置错误均保留。两个原pending源字节不变，已经晋升候选正控；当前完整语言及ZUI未完成，下一步见[完整缺口清单](LANGUAGE-REMAINING.md)。
