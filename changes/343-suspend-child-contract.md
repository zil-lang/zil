# 343 明确Suspend子调用公共合同

按既有§12.1–12.3扩展实际子帧和持有责任，具体语义见新增§12.1.2。
Wire/Typed37联合保留342已发布的life合同，不重用36字节域；125用于
SuspendCall，121–124保留给340的独立reservation命名空间方案。
源选择复用既有effects固定上界与resume(local)，不新增direct效果词。
公共表示仍未核；当前各消费者的实现/验证范围分别记账，不能把公共发布
当作完整Task、拥有结果、Waker、draining或跨线程实现完成。
