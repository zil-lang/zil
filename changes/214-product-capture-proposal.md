# 214–216：完整 Product 拥有捕获

目的：206 已支持嵌套组件状态的普通完整转交，但不能将其保存在持久事件闭包中，阻塞拥有多层状态的 ZUI 回调。实现依据为设计 §9.1.2；schema/Wire27 不变。

214 负责 Types 捕获分类、factory、State/外层 callback Store 资格及源码正反例；215 负责独立 Analyze 的责任槽、捕获来源和预算；216 负责独立 Verify、Lower/LLVM 内联布局及逐 child 清理。集成方管理规范、主树状态、旧测试迁移和全量验收；非作者审核绑定最终组合提交。

接受性增量：只允许206普通有限 Product 的完整拥有捕获，不开放其内部 Store/Closure/拥有数组/Sum/借入，也不把 Product 本身开放为 Store 元素。直接 child 一个责任槽与存活标志，内部递归清理复用206完整布局；捕获视图不授予 create 证明。

基线：8527fb393d1b6938ad9a4e5d21ad710d78a19ad7；旧 impl206_independent_capture 的 source-derived raw/Wire 已在本轮原样通过旧拒绝测试，日志 recovery/baseline206-capture.log。保留原夹具字节与既有206历史，迁移仅改具名合法控制预期；协调缺child/错Bind/共享写/错误flags仍拒绝。

验收：显式move只读Fn、默认整体/child消费、FnMut完整回填、FnOnce提前/错误退出、未调用销毁、不同实例标志、多层child、State/外层callback Store、源及独立raw/Wire最近反例、O0/O2本机运行、真实分配失败清理、原flat预算与新增N/N−1/fresh N。无Windows运行环境则明确NotRun，不能用Linux代替。
