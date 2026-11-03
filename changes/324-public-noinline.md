# 324 机器层NoInline边界

集成方批准并提供函数位置Attr::NoInline，API和文本后端沿现有attribute name路径消费。它不授予源语言权限，不改变ZIL调用ABI、LIR或Wire版本。只允许函数位置，参数/结果位置及重复属性仍拒绝。

324以私有copy/zero helper保持大值内存操作粒度，在用户请求的default<O2>下运行，不能强制O0或optnone。原a18的O0已有新实现成功，O2限额失败保留；本属性本身不等于O2问题修复，必须待相同源码两后端实际核回。
