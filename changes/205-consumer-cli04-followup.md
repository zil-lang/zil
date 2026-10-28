# 205 CLI04 消费者同源复测

首轮正式整仓发现旧完整 State 的 `(next)=relay(next)` 被误拒。修复后，root在clean `54452f8a50eb494c7c444bca3a9682802246284f` 实际构建CLI04，SHA `8cf2524495bc1fb1ac59fad705ead558c4c6f72fb200b5b21dc871ef22a5e323`，28,665,608字节。作者消费者在同一clean源码身份重新从ZIL编译运行，没有把CLI03成绩重标。

五模块全部源、consumer.py中的先定oracle、两个分配观察器与既有 `82c5cc1c` 逐字相同；所有当前生产输入及规范9813均在执行前核对。Linux原生O0/O2实际140次通过，Win64/Wine O0/O2另140次通过；每平台20普通、120分配观察/故障执行、88实际分配失败，全部最终live0，Wine bad_operations0。每条观察与对应CLI03原执行值相同，峰值最多4份live allocation、264字节请求payload；不把此量当进程RSS。

这次源与编译器都绑定54452。root随后 `2db3c090` 仅移除断言迁移后失去唯一调用点的旧测试helper，未加allow(dead_code)或改语言输入；原workspace Clippy Fail由root保留，此作者结果不替代其重跑。四处旧阶段迁移的原Fail、31/0有限复测和52原生控制另在legacy-phase收据中，不能重复相加。

精确小收据为 evidence/impl/205/consumer/CLI04.json。旧CLI03、原Expand/Types源拒绝、原Wine IPC启动失败等均按原身份保留。此结果是作者双平台消费者复测，非作者审核、正式整仓及最终集成分别报告。完整1.4 NotSatisfied、B0 Doing、产品和原生Windows NotRun保持。
