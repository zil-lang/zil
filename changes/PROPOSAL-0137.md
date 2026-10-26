# 137 恢复导入主干遗漏的 bytes_truncate

基线 1d7216f0c72f055a4e484f786a1a75a11f1cd12f。原实现来源是先前交付的 source.zip；这是移植与重新验证，不是新发明或新性能成果。

语义：仅允许0<=new_len<=old_len，无分配，无增长，不改变容量；非法长度保持全部旧状态。每次调用都保守视为写，跨调用的同backing借片必须被独立Verify拒绝。新增ByteBufferTruncate、raw tag69、schema/wire18、fingerprint V18；旧wire17必须拒绝。正文与interfaces同批声明，集成后才能成为main规范。

只提取原语纵切；保留当前main的跨模块record defaults、效果worklist、查询实现和默认额度。不得整文件覆盖回旧源码。原编号134现已用于project defaults，恢复测试使用137。

验证须重新构建当前编译器、运行源正反例、raw/wire突变、O0/O2原生短前缀矩阵；已有默认字段测试和资源窗口测试必须继续通过。独立审核尚未发生，不标为已合入。
