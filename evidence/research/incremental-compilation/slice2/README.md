# 组合编译效率专项证据

最终生产源码 `5cf1c417f5cbdc0895e5edceb3053d9d4da490c6`。结论与完整单元见 [RESULTS](../../../../docs/research/incremental-compilation/RESULTS.md) 和 [MEASUREMENTS](../../../../docs/research/incremental-compilation/MEASUREMENTS.md)。这是限定研究交付，非正式产品门或主线整仓验收。

| 目录/文件 | 内容 |
|---|---|
| initial | 首片→b230组合版本，保留512模块RSS及私有编辑回退 |
| memory-fix | 首片→646释放旧owners后的完整重跑 |
| final | 首片→5cf最终正确性修复版本 |
| main-final | 开工main21058d0e→5cf直接对照 |
| mixed | 八组真实语料O0/O2；旧byte_io O2错误组比值null |
| diagnostics.tar.gz | 原混合测试中止、O2根因、独立90组常量控制、跨二进制缓存、链接器探索 |
| validation | 精确功能审核、486项最终限定测试、环境/源码/工具摘要、历史日志及200,000例旧/新前置检查oracle |

独立统计复核收据为`validation/final-evidence-review.{md,json}`，全部1599数值字段、192完整表行与7主表行一致。其原始重算脚本、逐文件inventory和重算结果一并保留；该脚本含原环境路径和本机可执行产物检查，完整运行需要原环境或重建产物，不能直接在仅解压数据包后声称所有二进制hash重验通过。仅统计复算所需的原始行已完整交付。

每个性能目录包含可直接读取的manifest/summary及raw.tar.gz。压缩包保存原始rows.jsonl、完整CLI报告/stdout/stderr、源文件、LLVM IR和测试结果。Session源码由跟踪的session_probe.rs生成；每次native输出和verification_runs在打印计量行前断言，未另存每请求完整report/native stdout。Session行记录phase、wall和请求后RSS。

前两窗CLI二进制随后被最终构建覆盖；原manifest保存当时SHA和精确源码身份，不能在现存旧CLI上重新验证hash。旧probe在测量机器保留；可按源码重建，但不保证二进制逐字可再生。最终受测CLI/probe SHA可由现存文件重新核对。压缩包不包含可执行程序、LLVM共享库、对象或Rust增量缓存；这些排除项不冒称交付。

外层`MANIFEST.json`绑定本目录所有交付文件（自身除外），每个raw包的`FILES.json`绑定包内原始文件。验证包内内容可使用Python tarfile逐项读出并对照SHA-256，无需执行诊断shim。`raw.tar.gz.json`记录压缩包SHA、大小、原记录目录和排除规则。

重跑命令和计量边界在RESULTS；限定验证入口为`tools/research/incremental-compilation/verify_scoped.sh`，构建与性能工具同目录。日志中的首轮失败、重试和旧状态均保留，不按成功测试的新覆盖量重复累计。诊断C/Python/shim和机器IR oracle是对应层级的证据，不冒充真实ZIL源码程序。
