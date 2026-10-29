# 205 完整 State 的同步回调替换

五个实际模块组合204的合格叶Store拥有捕获、同F两字段State、普通完整T Store helper及205的一个State加独立replacement。`factory.active` 的A/B/C三份环境来自同一表达式类型，各自有叶Store、Ticket和可变counter；字段替换不合并它们的运行时身份。

输入0–7分别为反序/non-adjacent参数更新、一次事件后更新、同步取消分支、replacement工厂开始前的Copy参数失败、第三实参失败、反序实参失败、弱FnOnce消费旧字段后补入、continue/break事件序列后更新。取消只是在同步函数中返回原State；没有异步取消或 `E!State` 新权限。错误传播只发生在调用者准备有限Copy参数时。

每个有效输入先把完整State放进普通Store，再pop到拥有局部，更新后重新push；末尾真实扩容。正常更新先创建C并对其Store作一次真实分配/调用，再进入P、销毁旧A、发Q、调用C/B/C；取消路径K只关闭已准备C。消费控制U–V之间以FnOnce实际消耗A，返回的C仍在调用者原FnMut能力下可重复调用。Scope Guard Z与State头S区分准备临时和普通外层清理。

固定marker：N加标签是factory创建；A/B/C是成功调用；1/2/3逐个表示对应环境叶Store元素的清理；a/b/c是环境Ticket清理；S是State头；P/Q是替换前后；K是同步取消；E是Copy准备错误；U/V是FnOnce消费边界；F表示该消费内部的分配失败已处理；L是完整成功路径结束；Z是外层Guard清理。未执行机器前先固定源码和oracle，不用观察输出回填预期。

`tools/language/205/consumer.py` 复用真实realloc/HeapAlloc观察器，对Linux及Win64/Wine各执行O0/O2普通、零故障、每个实际分配点失败、最后分配点之后的控制和无效/EOF输入。算法不由C模拟；C只观测/注入分配。源码、全部命令、Core/Typed/Plan/Wire相关文本、LLVM布局与结果分别记录；机器产物不进入源码/证据交付。原生Windows及完整产品条件另列NotRun。

此文件登记消费者输入和冻结预期，不表示编译、native或平台矩阵已经通过。实际成功范围须引用对应受测提交、CLI摘要及运行收据。
