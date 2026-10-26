# 有限事件批处理

四模块普通 ZIL 消费者：通用 FnMut helper 顺序执行四个事件，callback 在真实外部累加器写回，错误通过 `?` 提前返回。正常输出 EEEEST、exit 0；将 main 的 stop 改成 3 输出 EEET、exit 1。T 来自 Ticket 的唯一 Drop；错误后不执行第四次回调。O0/O2 由 impl126_helper_sequences 的原生测试验证。

内部完整 trace 保留 1/2/3/4 次调用的普通 Return；错误 Return 与成功 Return 共享退出种类，次数不同仍分别公开。有限无环边界没有开放一般循环合同、转发 helper 或局部 factory 闭包直接调用。
