# schema8 图研究的功能集成结果

此项由root集成，另交非作者复核。正式性能仍固定历史schema7提交
`3d4375a18ea4ab9c4e800461befea1d7f3c69f42`，没有将3192个旧样本改记到schema8。

实际受测代码为`31de818`（完整SHA见[机器收据](../../../evidence/impl/055/schema8-integration.json)）；
本次15项研究测试、format与clippy检查通过。前一次干净`89be56e`执行的原始日志也保留，
后续补充只增加完整TypedIR/关系输出和schema8身份篡改反例，未改变生产编译器或研究求解器。

四个真实`examples/mainline/error_cleanup`模块逐文件解析真实import，经程序级Expand与Types合成TypedIR。
整个模块通过生产Analyze/Verify；全部19个非外部函数逐个抽图，比较独立TypedIR oracle、flat、cut和block的
所有原点活跃行与完整带标签事件关系，共853个图点、20处错误ScopeExit。没有跳过函数或扁平化源码。
详细数量、TypedIR指纹以及全部输出原字节保存在
[功能证据目录](../../../evidence/impl/055/schema8-shadow-final/)，而不只保留布尔成功标志。

三种错误值操作显式读取value；ScopeEnter/ScopeExit没有SSA操作数，仍是必保留的观察事件。
ReadErrorUnion的保守失败出口同时由候选抽取和独立raw审计分别维护。当前错误联合两臂均为Copy，
不增加Desc或借入闭包活跃槽；这不表示未来拥有载荷可以沿用此假设。

最近反例分别删除Error作用域事件、删除错误投影失败边、改错注入tag、把Error退出类别改成Return，
以及交换错误/成功分支目标；独立raw审计均拒绝。既有完整行、捕获身份、清理顺序和关联交换反例仍通过。
这些结果证明本次受限shadow保持其声明的功能关系，不是一般联合资源状态证明，也没有制造生产Verified权限。

复现功能测试可在当前源码运行：

```bash
cargo test --manifest-path tools/research/055/Cargo.toml --offline --locked
```

若要保留完整输出，先建立一个新的父目录，并把`ZIL055_SCHEMA8_REPORT_DIR`设为其中尚不存在的子目录；
测试会用独占创建避免覆盖原输出。历史性能须从配套Git bundle检出历史源码，并使用原manifest，
不能在schema8上跳过冻结输入校验。功能检查不提供新的性能、峰值内存或跨机器隔离结论。
