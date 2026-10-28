# 181 作者限定结果

受测源码8493aca；公共schema/Wire22与Rule1.4绑定，新的Store独占指令88保留手工字节、所有截断和各操作数指纹校验。旧opcode80–87不重新编号。Linux候选不能改为Win64使用，伪造位宽、字节序、后端、目标、算术检查和规则版本均在raw/Wire独立入口拒绝。

## 平台结果

只把两个Linux限定守卫改为现有精确checked x64 LLVM平台谓词；Win64沿用ByteBuffer已使用的GetProcessHeap/HeapAlloc/HeapReAlloc/HeapFree ABI。未混用POSIX allocator，失败不改旧block。Store单独存在也触发运行时，不依赖ByteBuffer哨兵。

Linux和Win64/Wine各72次执行：22个未经插桩的正式CLI原生程序、50个测试插桩程序，其中32个分配失败。5模块State入口覆盖0..3个运行时元素、替换准备失败、增长失败保持以及字段错误退出；单模块带padding、i64和固定数组的元素核对真实stride。失败push归还并清理完整元素，pop后尾槽不重复清理；每个heap probe实际allocated=frees且live=0。

Windows probe是独立派生测试二进制：仅将真实LLVM里的四个外部函数名重定向到记录shim，原LLVM和派生LLVM哈希同时保留；shim成功路径调用真实Windows Heap API。普通PE执行与probe计数严格分开。原生Windows主机验证未进行。

## 失败与复验

新Rust测试首次使用了不存在的wire函数名，编译失败且零测试执行，修正为实际encode_module_candidate。独审发现两个旧schema21断言未迁移，已保持原标签/截断内容并改为22、未知89/90。首Win64矩阵在第一个普通程序调用前Wine超时，零观察；隔离Wine与LLVM/Rust动态库路径后相同程序实际运行，第二矩阵通过。所有原日志保留，没有删除失败或替换成后验通过。

## 范围

这是公共与平台限定片；新178泛型、179字段精度、180独占元素的组合及完整workspace由root最终集成另验。全语言NotSatisfied、产品NotRun、父B0 Doing。此平台结果不证明编译提速、原生Windows合规或全部1.4语义完成。
