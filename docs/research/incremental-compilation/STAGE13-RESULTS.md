# Stage13：加速兼容的指纹计算，缩小暖编译差距

生产实现 `4bbbce204bb468d83eaebb78573c98104c3de348`；基线交付 `4cb3687127a1980b2ace965af9fdc8c8e5de93c5`，生产实现743c9699e。专项保持在 [PR #13](https://github.com/zil-lang/zil/pull/13)。

此前最弱的均匀8×1024函数编辑请求，本轮三组配对均改善：较Stage12平均减少 **16.4%**，新/Rust配对几何均值 **0.773**，三对分别0.752、0.767、0.802。中位数405.3ms对Rust511.2ms。这次在该固定样本中有重复领先证据，但不能推广为工业项目或所有场景稳定胜出。

动态分支暖请求仍是剩余缺口。为避免反复运行约一分钟的Rust冷O2，只在完整窗口之后，对已有编辑缓存做五组轮换暖请求：新版中位数 **269.4ms**，旧版340.7ms，Rust220.0ms；新/旧配对比 **0.814**，新/Rust **1.207**。五对均比旧版快、均比Rust慢。它们共享每臂一个已生成缓存历史，不是五个独立冷构建历史，也不与前面的warm-before-edit单元混合计算。

15个完整比较单元中14个新/Rust点估计更快，唯一仍慢的是动态分支暖请求。代价与不确定性保留：目标冷请求一对较旧版回退14.2%，动态分支单次冷请求回退3.0%；新增固定的registry依赖；无SHA硬件的性能本轮未测。

## 归因与实现

Stage12的同版本私有诊断，在动态分支暖请求中记录到约333ms完整请求、238.6ms后端。其中机器键共处理 **11,253,201字节**，累计key时间64.3ms，BufferedHasher向SHA更新的区间和41.9ms；均匀分布对应5,944,999字节、27.7ms key、22.3ms SHA。此前只说“哈希慢”还不够，这次确认了压缩核心在键生成中的主要占比。单次诊断的嵌套区间和跨CGU区间不能当作可相加的请求壁钟；不据此直接承诺完整提速。

本轮采取三个限定改动：

- `zil-base`内部SHA-256替换为精确锁定的RustCrypto `sha2=0.11.1`，关闭默认features，使用安全Digest接口。保留32字节SHA-256、原Hash字节流、所有缓存字段和协议；没有改用弱哈希、截短摘要或删除依赖。默认CPU检测在支持SHA/SSE4.1的x86上选择SHA-NI，其余走软件实现，不使用target-cpu=native。本机具备相关特性；这不构成跨平台性能结论。
- 机器预检的SSA类型、聚合索引与GEP游走借用当前不可变模型的类型，省掉深复制。原两轮完整验证、类型/操作数/支配检查、错误顺序和validate-only资格保持；不复用旧证明或跨模型schedule。
- checked-add链候选借用原块名称和类型形状，只有确定接受的替换计划才拥有名称。动态分支中许多候选最终不相邻，避免为这些失败规划先分配再丢弃；原形状、溢出、邻接、使用计数、额度及变换前完整预检都保留。

最终动态分支warm-before-edit的机器key累计区间57.1→24.5ms，预检39.1→36.9ms。它与完整请求改善方向一致；本轮没有分别消融三项改动，不能给每个改动独立的端到端收益百分比。普通完整编译和exact-object命中原本就各预检一次，未把保存schedule当作不存在的重复检查优化。

这次采用的是[固定RustCrypto源码](../../../evidence/research/incremental-compilation/stage13/source/RUSTCRYPTO-AUDIT.md)中的成熟压缩与CPU分派实现；Rust编译器的稳定查询/所有者组织仍见[Stage8固定版本审读](STAGE8-RESULTS.md#从-rust-源码落实了什么)。没有为图切割/zit新增未测的归因，也没有把整项目精确缓存表述为跨编辑函数级Types增量。

## 兼容边界与依赖代价

ZIL工作区的unsafe禁令保持；CPU探测和架构intrinsics封装在固定外部依赖。源审计记录版本、发布包校验、许可、调用与分派路径，不声称对RustCrypto完成新的独立密码学安全审计。根Cargo.lock由集成者管理，新增9个registry锁定包（含其他目标平台的依赖），内部依赖图和公共Wire未改。README与交付说明已补首次`cargo fetch --locked`或预备匹配缓存的条件，源码包不附Cargo registry缓存。

摘要字节兼容限于现有有界编译输入及FIPS消息长度域（少于2^64 bits）。旧自写实现显式使用wrapping长度运算；上游普通计数/乘法在Rust溢出检查开启时，累计达到约2^61字节的超长流可panic。不能声称这种超出标准域的公共FingerprintBuilder流仍保留旧wrapping行为。当前编译预算远低于该边界；没有新增输入限制或放宽预算，也没有用不可行的超大流测试掩盖区别。

## 完整请求比较

固定上轮LLVM DSO SHA256 `7b5c5ef8b204e019c58da7bbb01853dc56fa68fcec08364c5d07b10494fa9a98`，ZIL为LLVM23.1.4加既有四TU修复；Rust1.99.1固定提交58cf8f9c、LLVM23.1.1。三臂同机、按序列轮换、各自独立缓存，测完整CLI请求及链接/发布/缓存写入。源码生成、原生检查不计入编译时间。

ZIL显式O2、incremental-o2、after合并、8 CGU/2 jobs和可信语义/对象缓存；Rust O2、8 CGU、incremental、overflow-checks=yes、panic=abort。default是ZIL保留符号合同，closed为显式封闭；Rust私有helper更接近closed。LLVM版本/修复与导出合同差异公开，不声称优化器配置逐pass相同。

均匀8×1024保留符号3个独立cold→warm→edit序列；单8192、偏斜8模块、动态输入2048分支和均匀closed各1个保护序列。合计63有效构建/原生检查、42次当前ZIL Verify、21个未共用Rust观察、7个独立序列、15个比较单元。保护项只有单次点估计。

| 分布/合同 | 请求 | 序列数 | 旧ZIL ms | 新ZIL ms | Rust ms | 新/旧 | 新/Rust |
|---|---|---:|---:|---:|---:|---:|---:|
| large-even-8x1024 / default | cold | 3 | 1229.5 | 1194.7 | 1314.1 | 0.962 | 0.951 |
| large-even-8x1024 / default | warm | 3 | 268.9 | 232.1 | 421.7 | 0.860 | 0.536 |
| large-even-8x1024 / default | edit | 3 | 477.1 | 405.3 | 511.2 | 0.836 | 0.773 |
| large-8192 / default | cold | 1 | 1358.9 | 1145.9 | 2242.6 | 0.843 | 0.511 |
| large-8192 / default | warm | 1 | 280.7 | 246.2 | 409.2 | 0.877 | 0.602 |
| large-8192 / default | edit | 1 | 530.9 | 485.8 | 1351.0 | 0.915 | 0.360 |
| large-skew-8 / default | cold | 1 | 1221.3 | 1185.8 | 1468.1 | 0.971 | 0.808 |
| large-skew-8 / default | warm | 1 | 268.6 | 221.2 | 436.9 | 0.824 | 0.506 |
| large-skew-8 / default | edit | 1 | 481.0 | 409.5 | 799.1 | 0.851 | 0.512 |
| opaque-2048-branch / default | cold | 1 | 11583.1 | 11925.8 | 57383.1 | 1.030 | 0.208 |
| opaque-2048-branch / default | warm | 1 | 343.0 | 297.2 | 255.1 | 0.866 | 1.165 |
| opaque-2048-branch / default | edit | 1 | 12206.0 | 12075.0 | 58043.3 | 0.989 | 0.208 |
| large-even-8x1024 / closed | cold | 1 | 475.8 | 374.5 | 1324.8 | 0.787 | 0.283 |
| large-even-8x1024 / closed | warm | 1 | 191.5 | 184.8 | 423.2 | 0.965 | 0.437 |
| large-even-8x1024 / closed | edit | 1 | 390.9 | 385.2 | 514.2 | 0.986 | 0.749 |

耗时是中位数，比值是同序列配对几何均值，不能直接用中位数相除替代。目标暖请求新/旧范围0.837–0.877，编辑0.795–0.861；两者三对均改善。冷请求范围0.800–1.142，不能称稳定改善。目标CPU新/旧冷/暖/编辑为0.973/0.851/0.807；RSS没有实质变化，wait4峰值不是同时整个进程树峰值。

## 残余暖路径的集中复测

`stage13_warm.py`复用上述opaque编辑后的实际源文件、编译命令与缓存，三臂轮换5轮，共15次暖请求；实际clang路径/SHA、LLVM、编译器、CPU亲和性及源文件SHA均绑定。每个ZIL请求须观察到Analyze Reused、当前Verify一次、对象缓存enabled且8/8命中；每次运行原生输入7，对照已有编辑oracle。无效行先保留再中止，不输出成功汇总。

| 观察 | 旧ZIL | 新ZIL | Rust |
|---|---:|---:|---:|
| 暖请求中位数 ms | 340.7 | 269.4 | 220.0 |
| 新/本列配对几何均值 | 0.814 | 1.000 | 1.207 |
| 新/本列配对范围 | 0.739–0.914 | — | 1.174–1.251 |

全部15请求/原生检查有效，其中10次ZIL当前Verify、5次Rust；没有新的冷构建历史。新增产物15/15逐字等于各自seed。工具在finally恢复原ELF/.ll并核对源SHA，原完整窗口的质量输入未被暖复测替换。它提高对这一固定暖路径的观察密度，不是大项目分布或独立历史的置信区间。

## 验证、产物与交付

62个不同限定控制通过：base10、机器预检20、链规划13、对象缓存10、补丁记录5、真实源码原生4。强制软件SHA另执行同一base10组，通过但不计为新增不同用例；原标量实现仅保留在cfg(test)中作为独立差分oracle，标准已知摘要、流式边界、Clone及非破坏finish、完整缓存key/record兼容控制均保留。相关4crate all-targets Clippy、fmt、真实metadata架构检查通过，非作者审核绑定精确生产提交。没有本地跑正式全量。

七对既有编辑ELF的`.text`字节完全相同，动态分支和closed两对整ELF相同；其余5对差异仅位于包含编译器身份的私有符号字符串表，大小增量均为0。单大模块146个原始jmp标签差异完整保留，指向同址同正文的旧私有体。反汇编比较器的入口间接调用仍显式未完全比较；直接节字节证据不升级为一般ABI/运行/内存证明。Stage11保留旧合并thunk的既有跳转代价仍在，本轮没有新增或消除它。

当前交付全量由远端官方/研究LLVM CI执行，以最终提交工作流为准。先前4cb368712的官方41305054015、研究41305046739均已成功，只作基线记录。历史handoff完整性Fail保留；没有合入main或发布云环境。

## 剩余方向与四目标

在本轮固定样本内，均匀大模块编辑已重复快于Rust；动态暖请求仍慢约21%，完整冷/暖/编辑全场景领先没有成立。剩余成本集中于每次重新构造机器表示、完整调用闭包分区、LLVM文本产物及当前预检；对象命中不会自动消除这些工作。后续应继续按这些成本减少重复构造或建立精确绑定的原始工作产品复用，不能跳过当前Verify/预检或静默取消`.ll`产物合同。

跨编辑前端仍需要稳定声明/签名/效果投影及函数正文依赖；本轮未实现。当前样本覆盖大单体、均匀/偏斜模块和动态分支，但仍不足以代表泛型、CT、资源密集和工业项目。历史全景check/O0、更多生成族和14个小项目继续保持[Stage9版本身份](STAGE9-RESULTS.md)。

| 目标 | 实证与限制 |
|---|---|
| 安全 | 当前独立Verify、完整机器预检、额度和失败事务保留；声明固定外部密码原语边界及超长流差异，完整语言安全不升级 |
| 运行/内存 | 7对代码段相同、无代码体积增量；不替代与Rust的一般运行/RSS同级证明 |
| 语法/维护 | 源语法不变；减少复制，新增固定第三方依赖和首次获取要求 |
| 编译 | 目标编辑及暖请求重复改善，动态暖差距缩小但仍存在，冷请求有回退样本 |
| 至少一处突破 | 固定大模块编辑具有重复领先证据；全场景及产品级四目标合取未成立，任务Doing保持 |

证据：[身份](../../../evidence/research/incremental-compilation/stage13/provenance.json)、[完整目标](../../../evidence/research/incremental-compilation/stage13/final-target/analysis.json)、[保护](../../../evidence/research/incremental-compilation/stage13/final-guards/analysis.json)、[集中暖请求](../../../evidence/research/incremental-compilation/stage13/warm-followup/analysis.json)、[质量](../../../evidence/research/incremental-compilation/stage13/native-quality.json)、[独审](../../../evidence/research/incremental-compilation/stage13/REVIEW.md)、[原始记录](../../../evidence/research/incremental-compilation/stage13/raw.tar.gz)、[14个实际ELF](../../../evidence/research/incremental-compilation/stage13/native-elves.tar.gz)。
