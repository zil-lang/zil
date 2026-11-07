# Stage14：避开重复发现解析，动态暖编译接近 Rust

最终生产 `75178b1cfb42bfd37746b7573999d74d7d7668db`；对照 Stage13 交付 `5821120ac5952bcf799afe76de1030741d9ccb89`、生产4bbbce204。[PR #13](https://github.com/zil-lang/zil/pull/13)继续草稿/Doing。

最终动态分支编辑后的五组暖请求，新版每组都比Stage13快：新/旧配对几何均值 **0.795**（减少20.5%，范围0.761–0.830），中位数 **243.3ms** 对旧版316.2ms。Rust中位数 **239.2ms**；新/Rust几何均值0.965、范围0.668–1.154，只有2/5对快于Rust。Rust单次波动使几何均值和中位数对比不同，当前结论是接近追平，没有稳定超越证据。

## 实施与收敛

- 直接LLVM文本writer用String追加固定语法、类型和操作数，保留标准数字格式化和原公共Display。原formatter作为测试oracle，全部Ty/Val/Op、属性、变参、空结构、非法可打印模型及256字节转义逐字对照；`.ll`仍完整生成，也继续供Clang参考路径使用。
- 当前机器预检内复用小函数的索引、CFG行和支配数组容量。仅≤32块且参数+指令≤256；每函数重置所有逻辑状态，大函数另用临时空间，超过256容量的operand scratch在成功/失败返回后丢弃。检查算法、首错、调度资格及完整当前预检保留。
- 函数机器生成使用现有jobs，固定64函数批次、至少128函数才并行，源序合并结果与首个普通诊断，所有线程退出后再返回。最终进一步让不足128个非摘要函数或小型封闭收集保持串行。该门槛只识别已有常量结果摘要；参数化简单leaf仍可能并行，不能声称消除了所有轻量函数的线程成本。
- **单文件精确语义命中提前于导入发现解析。** 当前canonical路径、源码读取、字节费用、root-stem映射仍执行；只允许Native、非closed、无active query且原文完全不含`import`/`syntax`子串。注释/字符串误中只会回退。现有Store完整源图、路径、全Limits、target、build-inputs与编译器身份仍绑定，不增加持久协议。原始Typed/Analyze候选只解码一次，仍经过当前独立Verify及产物保护；损坏、缺失、限额/导入改变和拒绝重试按原规则，重试使用同一已读源码。

首个70ba1b271组合的五组暖请求仅改善5.5%，并有均匀模块CPU/RSS不利点；完整36请求及补充15暖请求均保留在`final-*`/`warm-followup`目录，明确属于中间版本。没有把它冒充最终751结果，也没有分别消融后给四项改动虚构独立收益。

这轮沿已有[固定Rust源码阅读](STAGE8-RESULTS.md#从-rust-源码落实了什么)，把精确有效性判定放到昂贵工作之前，并按独立工作单元组织生成；ZIL的独立Verify边界保持。后续可参考Rust的[WorkProductId](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/dep_graph/dep_node.rs#L232)，评估精确绑定的分区布局工作产品。当前还没有持久化跨编辑函数语义增量，没有新增图切割/zit收益归因。

## 完整请求与不利观察

沿用LLVM DSO SHA256 `7b5c5ef8b204e019c58da7bbb01853dc56fa68fcec08364c5d07b10494fa9a98`；ZIL为LLVM23.1.4加既有修复，Rust1.99.1/58cf8f9c、LLVM23.1.1。显式O2、incremental-o2、after、8CGU/2jobs与可信语义/对象缓存；Rust O2/8CGU/incremental/overflow-checks=yes/panic=abort。default保留符号，closed显式封闭，Rust私有helper更接近closed；管线和导出合同差异保持公开。

最终`confirmed-*`完整窗口33/33构建及原生检查有效，24次当前ZIL Verify、9个Rust观察，四组ZIL独立cold→warm→edit历史。每单元只有一次观察。动态目标没有重跑约一分钟的Rust冷/编辑，其Rust列留空，下一节重新测当前暖请求。数字为ms，新/旧与新/Rust越小越快。

| 分布/合同 | 请求 | 旧ZIL | 新ZIL | Rust | 新/旧 | 新/Rust |
|---|---|---:|---:|---:|---:|---:|
| opaque-2048-branch / default | cold | 12530.9 | 12360.1 | — | 0.986 | — |
| opaque-2048-branch / default | warm | 304.3 | 283.8 | — | 0.933 | — |
| opaque-2048-branch / default | edit | 12440.7 | 13197.6 | — | 1.061 | — |
| large-8192 / default | cold | 1253.7 | 1347.2 | 2339.9 | 1.075 | 0.576 |
| large-8192 / default | warm | 249.6 | 218.5 | 448.5 | 0.875 | 0.487 |
| large-8192 / default | edit | 483.1 | 457.2 | 1310.9 | 0.946 | 0.349 |
| large-even-8x1024 / default | cold | 1282.8 | 1286.6 | 1373.1 | 1.003 | 0.937 |
| large-even-8x1024 / default | warm | 248.6 | 227.0 | 442.5 | 0.913 | 0.513 |
| large-even-8x1024 / default | edit | 412.3 | 420.4 | 520.1 | 1.020 | 0.808 |
| large-even-8x1024 / closed | cold | 334.7 | 328.4 | 1443.9 | 0.981 | 0.227 |
| large-even-8x1024 / closed | warm | 157.9 | 172.8 | 480.9 | 1.094 | 0.359 |
| large-even-8x1024 / closed | edit | 344.5 | 489.3 | 519.8 | 1.420 | 0.941 |

保护项仍有不利点：单大模块cold+7.5%、动态edit+6.1%、均匀edit+2.0%；保留全部记录，不将单次点估计称为稳定因果收益。封闭edit最初+42.0%，因此只对此新增两个独立ZIL历史（12请求、12当前Verify/native，未重复Rust）：edit配对0.951/1.012、几何均值0.981，中位数344.2对351.4ms；cold均值比1.004、warm1.002。没有复现同等幅度的回退，原不利点仍保留，新增两对不与原Rust单次数据混合。

完整目标warm的wait4最大RSS为140.0→116.3MiB；但大模块default的部分cold/edit增加约4.7–6.9%，均匀warm增加3.1%，线程/容量成本没有被忽略。这些是进程最大RSS，不是同时整个进程树峰值。

## 复用已有Rust历史，重新测暖请求

`stage14_warm.py`复用最终两臂ZIL编辑历史和中间窗口的同源Rust编辑历史。当前Rust SHA和-vV、源字节、CPU亲和性与LLVM身份均绑定；当前ZIL两臂链接器路径相同并记录SHA，不声称已单独绑定Rust默认linker二进制。wait4计时函数与Stage13逐AST相同，所有15次时间都是新观察，未复用旧时间或新增Rust冷历史。

| 五组暖请求 | 旧ZIL | 新ZIL | Rust |
|---|---:|---:|---:|
| 中位数 ms | 316.2 | 243.3 | 239.2 |
| 新/本列配对几何均值 | 0.795 | 1.000 | 0.965 |
| 配对范围 | 0.761–0.830 | — | 0.668–1.154 |

15/15构建/native有效，10次当前ZIL Verify、5次Rust；所有ZIL均Analyze Reused且8/8对象命中，每次原生输入7核对既有oracle。15个新ELF均等于各自seed，finally恢复ELF/LLVM文件并核对源SHA。五组共享每臂一个缓存历史，不能充当五组独立冷历史。CPU新/旧几何均值0.853；本工具15个RSS读数都固定309.316MiB，与父进程提前哈希大型clang产生的高水位下限相符，不能据此推断编译器RSS持平，故不作暖复测内存结论。

## 验证、质量与边界

45项不同限定控制通过：首批30项，发现路径8项、语义缓存7项；jobs1/2真实源码控制复跑一次，共46次测试执行。相关all-targets Clippy、fmt、diff通过。非作者静审分别覆盖文本、并行、scratch、精确缓存入口和工具；正式全量留给最终推送HEAD的两套CI。上一轮5821120ac的官方41310369468、研究41310360631已成功，不能替代当前验收。

最终三个窗口加暖复测与封闭回查，共60有效构建/native、46次当前ZIL Verify、14个新Rust观察，三类时间分开报告。四对主窗口编辑ELF的`.text`逐字相同、大小增量0；opaque与closed整ELF相同，其余仅`.strtab`变化。单大模块146个旧私有跳转标签变化仍指向同址同正文。额外封闭回查的4个ELF逐字等于已检查的closed产物；无须重复反汇编。比较器仍保留间接调用等排除，有限字节相同不升级为一般ABI/运行/内存证明，Stage11旧thunk成本仍在。

当前样本覆盖单8192函数、均匀8×1024、动态2048分支及closed；没有追加历史全景check/O0、泛型/CT/资源密集或工业项目矩阵。单文件shortcut对导入项目不生效，跨编辑语义投影和分区布局复用仍待实施。历史handoff完整性Fail保留，Cargo.lock、公共Wire、默认额度、main、环境发布及产品门均未改。

| Rust四目标 | 本轮结论 |
|---|---|
| 安全 | 当前Verify、完整预检、路径/限额/失败发布保留；未扩大语言接受集 |
| 运行/内存 | 四对机器代码段相同；编译器RSS既有改善也有不利点，未建立一般Rust同级证明 |
| 语法/维护 | 源语法不变；新增私有渲染、临时空间和调度逻辑，使用现有缓存协议 |
| 编译/突破 | 动态暖路径较上版重复改善20.5%，已接近Rust；2/5暖对领先不足以宣称稳定或全场景突破，任务Doing |

证据：[身份与口径](../../../evidence/research/incremental-compilation/stage14/provenance.json)、[目标](../../../evidence/research/incremental-compilation/stage14/confirmed-target/analysis.json)、[暖配对](../../../evidence/research/incremental-compilation/stage14/confirmed-warm/analysis.json)、[封闭回查](../../../evidence/research/incremental-compilation/stage14/closed-recheck/analysis.json)、[机器字节](../../../evidence/research/incremental-compilation/stage14/native-section-bytes.json)、[审核](../../../evidence/research/incremental-compilation/stage14/REVIEW.md)、[原始记录](../../../evidence/research/incremental-compilation/stage14/raw.tar.gz)、[实际ELF](../../../evidence/research/incremental-compilation/stage14/native-elves.tar.gz)。
