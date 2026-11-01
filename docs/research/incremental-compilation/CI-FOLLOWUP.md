# 最终测试目标格式与lint补齐

首个最终PR头`fd223da98`的自动运行 [41167332074](https://github.com/zil-lang/zil/actions/runs/41167332074) 在Formatting步骤失败。本机随后按CI的目标范围检查，也发现指纹测试的`sliced_string_as_bytes` lint。此前486项限定测试与**库目标**Clippy的Pass仍保留原身份；没有把它们改称为全目标Clippy通过。

补丁仅改三处测试文件：后端别名断言换行；旧编码器测试包装函数换行；将测试中字符串切片再取bytes改为相同区间的bytes切片。冻结旧编码器的正文摘要保持，4项编码对照、2项真实源码指纹对照与1项后端别名控制均通过。无生产编译路径、接受性、机器产物或预算变化，不重标5cf1c417的已冻结性能窗口。

本机workspace格式检查及九个受影响库的**all-targets Clippy -D warnings**均通过。`verify_scoped.sh`的Clippy阶段相应改为all-targets，测试范围保持。原失败、修正后检查与7项重测日志在`evidence/research/incremental-compilation/ci-followup/`；重测不是新增7项覆盖。

生产实现及性能仍由5cf1c417、报告交付由ca32b4c5b的原独审绑定；本补丁另行绑定独审。最新托管CI结果以[PR13](https://github.com/zil-lang/zil/pull/13)的精确头提交为准，旧运行保留，不把其格式失败改写为成功。
