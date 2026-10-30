本目录是施工作者的限定测试证据，不是独立审核或整仓验收。RESULT.json 绑定 clean 源码提交 1cb2bf36049ffa1b30102586be7a63d07f385233。raw.tar.gz 保留同次实际阶段 IR、candidate/Wire、LLVM API 对象、链接与运行结果；artifact-manifest.json 核每个原始文件。源码、测试与工具自身在该 Git 提交。

10 组测试与 19 raw/Wire 对照、4 次原生编译、6 次 allocator 运行是重叠覆盖，不相加为测试总数。O0/O2 的 4 个真实失败路径均最终 live=0；正常路径同样核 exact 分配/释放。原始候选生成与独立 Verify 的权限职责分别记录。旧 capability 断言迁移、Store223 组合和独审仍由集成者处理。
