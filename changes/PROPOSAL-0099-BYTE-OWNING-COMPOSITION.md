# 099：字节拥有存储与既有Package/闭包组合（draft21）

2026-10-25（Asia/Shanghai）。本提案先于实现；用户继续授权ZIL语言施工，设计线问题以源码反例反馈，不等待无关研究。

## 问题与选择

094–096已经完成ByteBuffer的普通工厂、参数、动态窗口和唯一清理，但首片禁止它进入已有Package和拥有闭包，导致增长数据不能与现有可保存状态组合。099复用既有责任、移动和清理协议，不新增容器原语、隐式堆盒、RC或世代机制。

开放`package P { manages data: ByteBuffer; ...Copy字段 }`的完整构造、普通按值参数/返回、直接字段take/refill/替换及其已有错误出口。开放实际Managed ByteBuffer的拥有捕获，以及包含该字段的完整Package的拥有捕获和已有opaque工厂返回。显式move与既有正文give推导的拥有捕获使用同一责任合同。

## 安全与失败边界

拥有头仍为内联`{ptr,len,cap}`，不改变普通ABI或wire表示。临时准备阶段的失败只清理实际已经获得的责任；提交后旧位置不再拥有同一payload。Package和闭包字段的清理必须先清除外部live责任标志，再调用内建字节释放；不能只调用用户teardown而漏掉内建资源。

ByteBuffer字段Desc和借片追溯到实际Package/闭包载体。首片允许以整个实际shell作为保守根，不声称不同字段窗口天然不交。活动窗口排除相交写、取走、替换、whole move与销毁；在最后使用后恢复资格。FnMut推导和独立Verify都必须识别直接原语及普通Desc helper的写效果，形参名字不同不证明实际来源分离。

仍关闭ByteBuffer Desc借入捕获、返回借自闭包环境的裸slice、拥有错误载荷、普通拥有聚合、一般嵌套拥有元素、CT/FFI载体、自定义allocator和异步。Desc借入捕获需要把动态backing效果传到外部活片核；裸slice返回需要lending结果合同，不能仅删除前端封门。

## 接口与迁移

没有新增Type/opcode/字段或布局，沿用schema/wire17和Typed fingerprint V17。此次为既有表示的接受范围扩展，由新的规范输入摘要和当前实现身份绑定；旧17表示保持。原094–096历史提案和证据不重写，当前支持边界更新到`spec/interfaces.json`。不改变默认预算、产品门或Verify独立职责。

涉及Types、Analyze、独立Verify、Lower、LLVM及源程序消费者；所有消费者同批交付。root持规范/共享schema/登记，099作者只改派定代码范围，root非作者审核精确候选后集成。

## 最近反例与完成条件

实际源码覆盖工厂返回动态数据、重复调用拥有闭包、Package字段取走/补回/替换、准备后续参数失败、错误与提前退出、直接/Package捕获销毁。最近反例覆盖活片期间move/take/store/drop、同源helper写、局部/闭包裸片逃逸、重复责任和伪造raw字段。独立核不得调用前端求证函数。

机器观察使用真实ZIL→LLVM→native的O0/O2结果，观察payload内容及故障分配器的准确释放计数；原生Win64与Wine兼容执行分别计量。没有实际运行的目标不记完成。新增能力完成不替代B0、完整语言或Rust四目标产品验收。
