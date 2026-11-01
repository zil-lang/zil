# 第三阶段证据

生产实现a4bec09f；候选身份5e0613cc只增加真实源码测试，生产相同。main-final是开工main210→5e，isolation是阶段2最终5cf→5e。两窗各5区组1560请求，全部单元及不利结果见[当前表](../../../../docs/research/incremental-compilation/LATEST-MEASUREMENTS.md)。本阶段512模块私有修改隔离窗口+13.5%保留。

validation保存精确功能独审、测试提交绑定、独立预算/取消/源保护probe源码及输出、493项最终限定测试与构建/Clippy/fmt日志；历史重跑不累计成新测试覆盖。四新窗统一统计复核及完整库存位于相邻stage4/validation。

每个raw.tar.gz包含rows、原始CLI报告/输出、生成源、LLVM IR和Session stdout；内部FILES.json绑定每个文件SHA，外部MANIFEST.json绑定此目录全部文件（自身除外）。ELF/对象/库/增量缓存不打包。Session native通过probe内部断言记录，不另存每请求完整native stdout。CLI RSS为峰值，Session RSS为请求后值。

这些是限定研究证据，不是产品门验收；阶段3旧probe没有CPU字段，不能将阶段4CPU诊断追溯归给本窗口。
