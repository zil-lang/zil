# 编译效率专题：方法选择与适用边界

研究日期：2026-11-09。这里只记录方法判断，不把论文中的加速比当作ZIL结果。

## 首片与实际裁决

基线327eccfaf的incremental-o2对每个分区导入完整直接调用闭包，所有导入副本成本超过源码函数总成本则退回单模块。首片只在该特定失败上按8→4→2减少分区上限；不改变复制上限、LLVM优化、符号身份、安全核验或其它回退。

两分区纯调用图中每个源函数至多复制一次，因此可满足原总复制上限，但这不是净提速证明。真实288次完整CLI构建已经显示：256模块编辑改善约15%–18%，冷编译退化约11%、暖编译退化约19%，冷CPU约翻倍。候选保持研究状态，不合main；精确数据与裁决见RESULTS.md和native/42612992774/。

这个负结果区分了两个问题：避免单分区回退可降低部分编辑重编范围，但完整闭包复制仍扩大总工作量、机器模型和缓存命中后的检查成本。不能只优化CGU数量。

## 近期一手资料及在ZIL的落点

| 资料 | 可借鉴机制 | ZIL落点与限制 |
| --- | --- | --- |
| IRHash，USENIX ATC 2025，2025-07 | 在结构化IR上计算完整指纹；按命中概率、识别成本和被省去的工作评价缓存 | ZIL已有结构化对象键。先实测布局键、对象键、序列化及重复扫描，考虑共享本次请求的完整摘要；不删除ABI、布局、原子性或调试合同等必要身份。 |
| Reusing Highly Optimized IR in Dynamic Compilation，ECOOP 2025，2025-06-25 | 重放依赖观察，重验优化IR的特化前提和运行时引用 | 可参考精确依赖与重放，但论文是GraalVM动态编译，不能直接当作ZIL跨CLI全语义增量，也不授权复用旧Verify。 |
| LLVM MTPC RFC，2026-05-22 | 按调用图及强连通分量切分大模块，并行优化和代码生成 | 与当前关键路径直接相关。需要同时衡量复制量、最大分区成本、编辑稳定性和优化质量。该项仍是RFC，不能当作已发布成熟算法或直接照搬作者收益。 |
| Inside Zig's Incremental Compilation，2026-07-28 | 分开布局、声明类型、编译期值、函数体；结果不变就停止传播；文件级ZIR持久缓存 | ZIL需显式覆盖trait/provider/life/CT/责任及导入歧义依赖，不能按函数体hash简单裁剪。Zig自研后端和集成链接器的毫秒级结果不等于LLVM O2完整CLI结果。 |
| GCC15增量LTO及cache分区 | 按相关符号聚集并兼顾分块工作量 | 后续稳定分区参考，不是简单增大CGU数量。 |
| TPDE，CGO2026，2026-02-03 | SSA适配器与低遍历次数的基线后端 | 对照是LLVM O0，不是发布O2；当前不替换用户要求的LLVM默认路线，也不把论文倍数当ZIL成绩。 |

## 下一轮优先级

1. 首先补因果观测。当前CompileReport只有总seconds、query_events和后端对象时间，不能仅凭Types=executed断言函数memo未命中；也不能把旧Stage15→53e的18%–33%回退直接归责给前端某阶段。
2. 后端按调用图/强连通分量及编辑依赖建立稳定分区成本模型，目标同时包括总复制成本、最长编译关键路径、源码修改影响的工作产品数。选择性导入/跨模块摘要需要独立合同及原生运行质量门，不能假称仍沿用完整闭包合同。
3. 检查当前对象键、布局键和机器预检里的重复扫描，只有阶段证据证明在关键路径时才改。缓存命中率提高并不等于整体耗时降低。
4. 将更多前端结果按真实依赖分开：声明接口、类型布局、泛型/trait选择、CT值、函数体、借用/责任摘要。以结果变化决定传播；不删必要依赖、不复用旧核验授权、不抬预算制造通过。
5. 编译器级无修改复核与构建工具级Fresh分别验收。后者可在没有输入/工具链变化时不启动编译器，但必须绑定完整项目输入、输出和副作用合同，不能简单看文件时间戳。

本轮不以更高缓存命中率代替低墙钟，不以两个分区代替收益，不把Check当EXE构建，不把Session当跨CLI缓存，不把直接rustc暖构建当Cargo Fresh，不削减LLVM优化换取损害生成程序运行性能的编译优势。

## 一手资料

- https://www.usenix.org/system/files/atc25-landsberg.pdf （正文与截图已核）
- https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ECOOP.2025.25
- https://discourse.llvm.org/t/rfc-thinlto-multi-thread-parallel-compilation-for-large-modules/90877
- https://mlugg.co.uk/posts/incremental-compilation-internals/
- https://ziglang.org/devlog/2026/
- https://gcc.gnu.org/gcc-15/changes.html
- https://gcc.gnu.org/onlinedocs/gcc/Optimize-Options.html
- https://2026.cgo.org/details/cgo-2026-papers/43/TPDE-A-Fast-Adaptable-Compiler-Back-End-Framework
- https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html
- https://clang.llvm.org/docs/ThinLTO.html
