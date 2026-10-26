# draft12 集成：有限拥有包与联合关系研究

2026-10-24（Asia/Shanghai）。承接 draft11 `6acda08c72dbd3e387893b4260770f0834dcdbe0`，用户明确授权保存并要求图研究与语言主线并行。

## 本轮已集成的语言结果

060公共表示/LLVM/native、061源码/Analyze、062独立Verify沿同一proposal并行，最终候选分别为 `e766fb225ad3b79ef079e51bee83d0f36fccddbc`、`3b0e926985d3fd2837aff15dc5e11dbbbc71c8ea`、`c7bccd322c9528267b6fd9cc32ce5de1cefa7c8b`。060由非作者error_verify审，061/062由非作者root审。各exact候选均以no-ff进入main，审核收据列出作者/独立执行及各自身份，不借共享依赖互相自审。

完整package的真实prepare guards、原子提交、普通Value整体移动、Copy/Desc字段访问及声明序清理贯通。三入口18输入在ZIL/Rust O0/O2上共72次执行；独立6次布局/返回近邻另记。前端44源码/raw/预算测试与Verify34新增测试按各自单位记账。首次多模块接口panic和旧预算回归保留；修复未改变旧预算阈值。

## 公共表示与规范身份

schema/wire9，Package type tag17，四操作49–52，typed fingerprint V9。060提案先于修改；主树单写spec/interfaces及新E-VERIFY-PACKAGE诊断登记。仅更新正式runner的能力scope文字，未修改通过数量/性能门或产品门。历史实现票保留其原规范快照，当前正式任务索引更新当前规范hash。

059研究最终候选5d1c1e79056d461669420861c8330fc3933a8ea4使用显式冻结draft11/schema8，算法/driver为c0c1dc7；经error_frontend独立审核后no-ff合入。43函数903次正式完整关系核对，当前cut/wire未建立稳定总成本优势、NoGo入核；两个预算失败入口均保留。原始portable archive及完整轻量独审已归档；精确成本结论以研究最终收据为准。编译器安全核仍采用生产独立Verify，实验不制造Verified包装或私自扩大接受集合。

## 集成验收与交付身份

正式门由root在本轮源码与证据集成、主树干净后执行；其精确source_commit、32个必要门及Rust测试数由evidence/final-summary.json记录。本文件不预填验收结果。打包HEAD只允许其后的结果/文档/交付元数据变化，并由PACKAGE.json与包外冷恢复报告分别绑定。B0继续Doing，完整语言NotSatisfied，产品NotRun。

下一主线先通过非逃逸具体环境的高阶helper，再接可保存处理器与平台绘制边界。当前package_ui是同步有界文本事件环；完整GPUI级窗口/GPU/字体/异步平台尚未实现。
