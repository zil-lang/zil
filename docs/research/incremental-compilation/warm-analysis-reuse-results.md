# Stage12：减少暖编译分析和当前输入复制

最终生产实现 `743c9699eda7b5ef79b0759702cb48a17acfb83d`，基线交付 `38a7d5a088ecd291255907dfca32e37785b8292c`（生产实现082efcfc）。专项保持在 [PR #13](https://github.com/zil-lang/zil/pull/13)。

本轮明确收益在**暖编译**：目标均匀8×1024函数的三组配对耗时较Stage11减少 **16.7%**，三组均改善，中位数323.1→275.6ms，Rust为427.4ms。编辑配对平均减少 **5.1%**，但一组回退；新/Rust几何均值 **1.006**，中位数503.4ms对493.1ms，仍接近持平，不能称稳定超越。

冷编译目标配对平均减少7.9%，但受第一组基线偏慢影响，三组中也有一组回退。动态分支暖请求仍是明确缺口：390.2ms对Rust204.1ms，约 **1.91倍**；其单组相对上一版回退8.5%，编辑回退3.5%。最终选定的15个比较单元中13个点估计快于Rust，只能说明这些受限样本，不能称全场景胜出。

## 为什么改变实施重点

先前对象函数补丁已经把均匀大模块的编辑请求拉到接近Rust。首个Stage12候选 `f15cf54a43a162f0c13fc11b8ddb26c344fc1315` 只减少快照哈希、Types形状解析复制及独立类型遍历的小分配。完整63请求窗口没有可靠净收益：目标冷/暖/编辑新旧比1.042/1.034/0.999，目标编辑新/Rust1.089。原始目录名为final-*，但它们是未选中的首轮窗口，完整保留，不能与后续selected-*拼接成最终样本。

随后私有诊断构建确认：首轮候选在目标编辑的Types约49→37ms、补丁正文哈希跨单元累计区间约6.8→0.5ms，确有局部工作减少，但占完整请求比例不大，不能单独当净提速证明。目标暖请求仍有约36ms Analyze；动态输入2048分支暖请求的Analyze约22ms、后端合计约256ms。该动态样本的冷/编辑ZIL约12秒、Rust约57–59秒，实际落后在暖请求，而非O2优化总时间。

诊断只各一次，前后不同时窗口，嵌套计时不能重复相加，跨单元snapshot时间是各单元计时区间之和而非请求壁钟。源码分别由38a7d5a08和f15cf54a43加提交内 `stage12_profile.py` 重建，计时输出不用于最终性能比较。

## 最终实现

- 可信本地语义缓存的现有Wire原本编码TypedModule和空CandidatePlan，加载时丢弃后者。现在原子保存完整成功请求实际采用的TypedProgram和原始CandidatePlan；完整源码图、路径/ID、全部limits、target、build inputs和安装编译器身份继续进入键。只升级私有magic/namespace至v2，公共Wire不变。
- 命中在原Analyze查询位置以当前Types snapshot为依赖记录一次Reused，随后完整执行当前独立Verify。没有持久化Verified能力。错误候选清理后从当前源码重建并保留两次Verify事件/计数；取消不重试，后端失败不重复链接，失败请求不发布缓存。记录超限仅放弃可选缓存，不扩大预算。
- PreparedProgram的严格标量子集在完整原预算、深度和诊断顺序下借用当前Core进行形状解析，避免复制完整树。未知形状从原limits回退原解析器；所有后续Types/Analyze/Verify继续执行。
- 补丁快照使用可重置缓冲哈希，保持旧摘要字节流和记录格式；借用函数名并按已检查长度预分配。Analyze/Verify各自的类型遍历仅把初始根节点留在栈上，后续遍历、计费和首错顺序不变。
- Expand名称查找借用现有字符串，Restore阶段省掉恒返回空的查询；保留完整节点遍历、名字作用域、隐私检查和预算。后端四处事件构造移动已消费的参数/实参所有者，去掉重复深复制，机器操作与ABI不变。

借鉴的是Rust的计算结果复用、稳定输入身份与当前依赖绑定，以及已有[固定Rust源码审读](STAGE8-RESULTS.md#从-rust-源码落实了什么)中的共享所有者组织原则。这里仍是整项目精确命中，不是Rust式跨编辑函数查询或红绿等值传播。没有把历史图切割/zit的效果重新归因给本轮；这些算法的既有证据与限制仍见Stage6/7。

## 测量与适用范围

固定Stage11 LLVM DSO，SHA256 `7b5c5ef8b204e019c58da7bbb01853dc56fa68fcec08364c5d07b10494fa9a98`；ZIL LLVM23.1.4加既有四TU修复，Rust1.99.1固定提交58cf8f9c、LLVM23.1.1。没有新增LLVM修改。两臂同一机器、交替顺序、独立缓存，包含完整CLI请求、链接、发布和可选缓存写入；实际源码与原生oracle相同。

ZIL显式O2、incremental-o2、after合并、8 CGU/2 jobs及可信对象/语义缓存；Rust O2、8 CGU、incremental、overflow-checks=yes、panic=abort。default是保留ZIL符号合同，closed是显式封闭合同，均不是所有CLI默认选项；Rust私有helper的可见性更接近closed。既有工具链与导出合同差异继续公开，不声称二者优化器逐pass相同。

最弱均匀8×1024分布3个独立cold→warm→edit序列；单8192、偏斜8模块、运行时输入2048分支和均匀closed各1个保护序列。最终63请求、42次当前ZIL Verify、21个未共用Rust观察、7个独立缓存序列；原生检查覆盖固定生成输入，动态分支使用输入7。其他动态输入、溢出、间接/闭包调用由限定控制覆盖，不当作额外计时样本。保护序列只有1组，不能据此宣布稳定回归或全场景胜出。

| 分布/合同 | 请求 | 序列数 | 原ZIL ms | 新ZIL ms | Rust ms | 新/原配对比 | 新/Rust配对比 |
|---|---|---:|---:|---:|---:|---:|---:|
| large-even-8x1024 / default | cold | 3 | 1296.8 | 1258.8 | 1351.8 | 0.921 | 0.936 |
| large-even-8x1024 / default | warm | 3 | 323.1 | 275.6 | 427.4 | 0.833 | 0.640 |
| large-even-8x1024 / default | edit | 3 | 512.0 | 503.4 | 493.1 | 0.949 | 1.006 |
| large-8192 / default | cold | 1 | 1546.6 | 1555.8 | 2249.3 | 1.006 | 0.692 |
| large-8192 / default | warm | 1 | 345.5 | 298.7 | 447.1 | 0.865 | 0.668 |
| large-8192 / default | edit | 1 | 670.5 | 549.9 | 1331.1 | 0.820 | 0.413 |
| large-skew-8 / default | cold | 1 | 1351.2 | 1208.9 | 1539.3 | 0.895 | 0.785 |
| large-skew-8 / default | warm | 1 | 355.4 | 264.8 | 454.4 | 0.745 | 0.583 |
| large-skew-8 / default | edit | 1 | 689.1 | 567.4 | 767.2 | 0.823 | 0.740 |
| opaque-2048-branch / default | cold | 1 | 12097.1 | 12062.3 | 53783.0 | 0.997 | 0.224 |
| opaque-2048-branch / default | warm | 1 | 359.6 | 390.2 | 204.1 | 1.085 | 1.912 |
| opaque-2048-branch / default | edit | 1 | 11328.5 | 11719.8 | 56056.2 | 1.035 | 0.209 |
| large-even-8x1024 / closed | cold | 1 | 405.3 | 379.4 | 1344.9 | 0.936 | 0.282 |
| large-even-8x1024 / closed | warm | 1 | 210.1 | 187.0 | 477.9 | 0.890 | 0.391 |
| large-even-8x1024 / closed | edit | 1 | 428.3 | 380.0 | 538.8 | 0.887 | 0.705 |

表中耗时为中位数，比值为同序列配对几何均值，因此比值不一定等于中位数之比。目标新/旧配对范围：冷0.811–1.036，暖0.752–0.933，编辑0.861–1.101；完整CPU新/旧比为0.929/0.807/0.944。目标编辑最大进程RSS中位数198.9→200.0MiB，暖135.8→135.4MiB，没有宣称内存改善。wait4 RSS不是同时整个进程树峰值。

所有7个候选暖请求均观察到Types Reused、Analyze Reused、当前Verify Executed；不是仅根据缓存目录推断命中。动态分支暖请求机器对象全8个命中，仍需当前机器构造、键生成和预检；该单次记录键计时累计85.7ms、预检52.3ms（旧57.7/39.7ms）。不能仅凭一组时延把回退归因于某个代码修改，保留该未解决结果。

## 正确性、产物质量与交付

f15范围40项限定控制通过；后续范围19项不同控制通过（4语义缓存unit、7真实源缓存、5模块/alias/privacy/预算、3动态/间接/闭包原生），总计59项不同控制而非全仓。伪造候选保留Typed/模块指纹，只修改initialized_out，实测Analyze Reused后被当前Verify拒绝，再执行Analyze并修复缓存，Verify计数2。初次后续控制runner漏LLVM API特性，有3个测试文件执行0项且Clippy被旧条件导入挡住；完整记录保留于controls-without-api-feature，补齐特性后所有19项实际执行，未把零项计为通过。相关all-targets Clippy -D warnings、fmt、diff检查通过，非作者审核绑定最终生产提交。

七对既有编辑产物完成离线静态比较：canonical全局集合一致，可比有效正文差异均为0；单大模块保留146个原始指令文本差异，逐项确认仅单条jmp的私有旧符号拼写改变（包含编译器身份绑定的action key），canonical地址/大小、目标地址与有效正文相同。其余6对原始规范化指令差异为0；ELF、.text、canonical符号及私有旧符号大小变化均为0。独立只读ELF节检查另确认7对`.text`字节完全相同，动态/closed两对整个ELF相同，其余5对所有差异字节只在`.strtab`。收据和脚本随原始包保存，14个实际ELF另存[原生产物包](../../../evidence/research/incremental-compilation/stage12/native-elves.tar.gz)。反汇编比较器中每对仍有1–2个入口相关间接调用形状标记为未完全比较，未计为完全通过。相同规范化指令/大小不证明一般运行时间或内存同级；Stage11相对完整重编保留旧合并thunk的跳转代价仍然存在，本轮没有消除或新增该代价。

正式全量由远端CI执行，历史handoff完整性Fail继续保留。Stage11交付38a7d5a08的官方CI41290206784和研究CI41290201058已成功，仅作先前版本记录；当前提交状态见PR，不能借旧CI宣称本轮全仓通过。没有合入main或发布环境。

## 剩余差距与四目标

下一轮的首要目标应是动态分支暖路径的当前机器工作，而不是继续大规模重复已领先的冷O2样本。其8个对象全部命中仍重建、分区、哈希及预检全部机器表示；先围绕这些实际成本选择可复用的原始查询结果和更低成本的摘要构造，保持当前验证与精确失效。当前自写SHA256和机器哈希的成本值得单独归因，不能未经测量就承诺换哈希会提速或降低摘要强度。

编辑请求仍因整个项目源输入键变化而重新执行Expand/CT/Types。真正跨编辑的前端增量还需稳定声明/签名/效果投影、函数正文依赖和当前ID/位置绑定；增加图边或删除输入hash不能替代这些前提。本轮没有实现该机制，也没有工业项目全景结果。既有check/O0、全生成族及14个真实小项目结果保持其[Stage9版本身份](STAGE9-RESULTS.md)，不自动升级为当前实现成绩。

| 目标 | 本轮实证与限制 |
|---|---|
| 安全 | 当前Verify、完整预算/诊断、缓存拒绝回退和事务边界保留；完整语言安全未升级 |
| 运行/内存 | 本轮不改机器算法或优化质量；有限原生与静态比较不能代替一般运行/RSS同级证明，Stage11保留旧thunk的运行成本仍在 |
| 语法/维护 | 源语法不变；增加严格标量解析快路与原始分析持久化接线，保留原路径回退 |
| 编译 | 具体收益与回退见上表；仍非全场景稳定超过Rust |
| 至少一处突破 | 有界工作产品复用继续积累实证；工业项目和产品级四目标合取仍未建立，任务Doing保持 |

证据：[身份与范围](../../../evidence/research/incremental-compilation/stage12/provenance.json)、[目标](../../../evidence/research/incremental-compilation/stage12/selected-target/analysis.json)、[保护](../../../evidence/research/incremental-compilation/stage12/selected-guards/analysis.json)、[封闭保护](../../../evidence/research/incremental-compilation/stage12/selected-closed-guard/analysis.json)、[质量](../../../evidence/research/incremental-compilation/stage12/native-quality.json)、[独审](../../../evidence/research/incremental-compilation/stage12/REVIEW.md)、[含负结果的原始记录](../../../evidence/research/incremental-compilation/stage12/raw.tar.gz)。
