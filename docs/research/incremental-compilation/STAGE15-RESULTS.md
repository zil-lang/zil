# Stage15：四项编译优化、完整成本与主线集成准备

生产 `c4d7b3fe1488fd22dc4f64e092b75a6ec2f08cad`；中间实现 `1222448ba0c8fae17fe60b3556a20f2e01476bae`；对照为Stage14生产75178b1cf。最新main 1cb1b62a3的17个记录提交已通过074015c3f合入本分支，无源码冲突。用户授权验收后合入main并删除已合并专项分支，当前报告不提前宣布远端集成完成。

## 实现与边界

1. 精确分区布局工作产品：保存有界索引布局，绑定完整当前机器模型、LIR规划输入、编译器和选项；命中跳过引用图/分组/调用闭包规划，保留当前声明和单元完整预检。实际缓存目标纳入ArtifactPlan保护；可选缓存冲突仅丢弃pending，不改变成功产物。只成功发布后写入。
2. 多模块导入发现复用：当前重新读取每份源码并重新解析每条导入的文件系统候选，检查新增歧义、缺失、符号链接和根目录限制，再使用既有完整语义缓存；免去暖请求重复解析，独立Verify保持。
3. 按实际64函数批次选择并行：紧凑标量叶识别一次，保存请求内Prepared；根据剩余普通工作及最大批次之外的工作决定线程数，保留源序/诊断/清理。不是增加线程数或预算。
4. 函数级Types增量：分离完整声明/导入/可见性/签名/ID/外部ABI上下文与函数体键，复用未变标量函数CFG，重绑当前位置、精确重放六项共享费用；限额不足原builder回退。一个打包epoch支持跨CLI复用，损坏拒绝同源重做，失败/取消回滚。最终只保留非平凡函数：至少四条顶层语句或顶层分支/循环。一般资源、泛型和闭包的跨编辑依赖图尚未实现；Expand、CT、全局检查、Analyze、Verify仍当前。

前两项借鉴Rust的有效性检查前移及工作产物复用，第四项落实函数计算与全局声明依赖分离；不是声称已实现rustc全部red/green查询或全语言语义增量。此次没有新增图切割/zit独立收益归因。

## 完整请求结果

固定研究LLVM23.1.4组合及Rust1.99.1，命令、实际compiler/DSO SHA、CPU亲和性和源码均在manifest/raw中。O2、8CGU、2jobs、显式语义/对象缓存、incremental-o2、after合并；表中的default是保留符号合同，**不是裸CLI默认配置**。

四种分布每种三个独立编译器历史，分别cold→warm→edit→edit-warm，共48次全部成功且native校验通过；32次ZIL当前Verify、16个新Rust观察。每单元仅一次观察，不作为稳定均值或工业全场景结论。edit-warm复用同一编辑后的缓存历史。

下表为ZIL/Rust耗时比，小于1表示本次ZIL较快：

| 分布 | 冷编译 | 暖编译 | 编辑 | 编辑后暖编译 |
|---|---:|---:|---:|---:|
| large-8192 | 0.662 | 0.310 | 0.293 | 0.321 |
| large-even-8x1024 | 0.977 | 0.322 | 0.644 | 0.334 |
| large-skew-8 | 0.789 | 0.306 | 0.531 | 0.336 |
| opaque-2048-branch | 0.210 | 1.081 | 0.204 | 1.045 |

主窗口16对观察中14对ZIL较快。动态分支的暖编译213.5对197.6ms，编辑后暖编译245.3对234.8ms，仍慢约8.1%/4.5%；不能宣称全场景超过Rust。三个大模块分布暖请求约为Rust的31–34%。

相对Stage14，四分布warm改善约14.7%/30.6%/47.6%/9.5%，edit-warm改善19.8%/27.0%/33.2%/8.6%。保留不利点：单8192函数cold+15.1%、均匀8×1024编辑+8.3%；不把单次波动全部归因为实现。额外closed保护历史8次全部成功/currentVerify/native：cold0.987、warm0.817、edit1.012、edit-warm1.168（新/旧），最后一项不利点保留；未追加Rust closed重测，也不把历史Rust数字混合进来。

最终共56次完整请求、40次当前ZIL Verify、16次Rust。五组最终ZIL编辑后ELF的`.text`逐字相等；不宣称新的运行时或代码体积收益。closed和主窗口都是生成工作负载，未补check/O0、泛型/CT/资源密集或工业项目矩阵，历史结果不得重标为本次性能。

## 中间负结果与收敛

1222448的32次screen和12次针对性warm复查全部留存。初版为简单leaf也保存约4.8MB函数epoch，编辑路径出现4–15%不利点；最终增加计算工作量门槛，均匀分布epoch降至约1.38MB，避免把易重建的函数写入重复缓存。最终收益只按新的c4d7b3f窗口报告，不叠乘历史收益或给四项组合虚构各自独立提速。

初版opaque edit-warm+25%后只补两场景三对暖请求：single三对均改善，opaque三对0.983/1.173/1.020、中位数255.9对256.4ms。未发现O(items×units)新增算法；没有据单点继续调整布局策略。完整原模型哈希仍是布局缓存成本，未来可研究与对象键共享完整逐item摘要，不能以不完整LLVM文本替代身份。

## 验证与交付

175项不同限定Rust控制覆盖后端、导入发现、函数Types、语义/对象缓存及真实原生清理；含修正后复跑共187次Rust控制执行，另5项Python恢复控制（两套CI也执行这些控制）。受影响all-targetsClippy和最终fmt通过。初次Clippy的dead-code/数组分块和后续类型/模块顺序修正记录保留，终态见provenance controls_final；测试数量不替代语义或性能结论。非作者审核绑定c4d7b3f为Pass。

014c历史两CI的workspace3265/reference3170/text-reference64均通过，最终失败原因是GitHub artifact额度耗尽，失败不改标。新CI保留全部测试和上传失败状态，提供有界、去重、校验和分块的日志恢复通道；集成前必须从最终exactrun下载后重建并核验完整归档，不能把本地emit成功冒充远端保全。研究LLVM改为main和PR触发；官方与研究工具链继续分别验证，合并不把研究LLVM自动变成默认安装。

Rust四目标：独立核验与语言接受性/预算保持，安全证据限于控制覆盖；机器`.text`一致，不新增运行/内存优势结论；语法不变、维护成本未测；编译成本有上述限定实证和不利点。完整语言、产品四目标合取及工业全场景突破均未建立，原product-gates不变。

证据：[比较](../../../evidence/research/incremental-compilation/stage15/comparison.json)、[身份与验证](../../../evidence/research/incremental-compilation/stage15/provenance.json)、[机器节](../../../evidence/research/incremental-compilation/stage15/native-sections.json)、[独审](../../../evidence/research/incremental-compilation/stage15/REVIEW.md)、[完整原始记录](../../../evidence/research/incremental-compilation/stage15/raw.tar.gz)。源码、输入、native ELF、LLVM文本、stdout/stderr和执行命令保存；可重建对象/语义/Rust缓存排除。
