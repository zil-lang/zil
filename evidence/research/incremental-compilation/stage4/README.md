# 第四阶段与最终候选证据

生产实现9fe5f55e。main-final为main210→9fe，isolation为阶段3生产a4→9fe，每窗10区组3120请求、2720次当前ZIL Verify、1240次native控制。mixed为八个真实示例O0/O2，共320测量构建/native及16个clang参考构建/native。旧byte_io O2有10个错误结果，保留原输出，速度比null。

结论及限制见[CURRENT](../../../../docs/research/incremental-compilation/CURRENT.md)，所有176+16新墙钟单元及48CPU单元见[LATEST-MEASUREMENTS](../../../../docs/research/incremental-compilation/LATEST-MEASUREMENTS.md)。主/隔离墙钟不利分别5/44、4/44；真实语料有效单元中6/15不利，不能只选暖请求代表全部编译。

validation包含精确9fe独审、路径差分oracle源/输出、495项限定测试/Clippy/fmt/构建日志，以及四新窗独立统计复核和文档核对。独审未重跑基准；三份Session probe同源控制用9fe源和保留rlib复构，ELF SHA与测量身份逐一相等。独立原始inventory包含本机ELF SHA，归档不包含ELF；完整脚本的二进制检查需要原工作区或复构，不能在仅解压档案后冒称已重验全部二进制。

raw.tar.gz内FILES.json绑定所有交付记录（rows、CLI报告/native输出、源、IR、Session stdout），包旁receipt绑定压缩包。外部MANIFEST.json绑定本目录全部文件（自身除外）。归档排除ELF、对象、共享库、bitcode及对象/Rust增量缓存。Session native是内部断言，不伪称每请求独立输出文件；验证次数在输出行前断言。CPU包括编译器线程，不含子链接器/原生执行；CLI RSS是峰值，Session是请求后观测。所有原始行保留。

阶段2RESULTS/MEASUREMENTS及其独审保持旧身份。本目录不改变正式产品门、规范、父工单状态或主线合入权限。精确文档交付提交的独立绑定收据放在父目录delivery文件中，避免manifest自引用。
