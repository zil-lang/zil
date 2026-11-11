# 严格预算与完整可见性对照：非作者原始结果复核

结论：**本次当前Rust证明有效，实际prelink跨owner Required已全部成为`.3.import`中的body definition，仍在两case运行质量门NoGo。** N64 B/A=1.6181061285，N256=2.0561126545，均超过1.10；Rust/B通过不能救相对回退。这是完整执行后的NoGo，不是Incomplete、无导入、预算放宽或工具错误。旧default无导入NoGo继续以原身份保留，不被本结果覆盖。

本审只读现有raw、重哈希和纯数学复算，未编译、执行native、调用LLVM工具或新增计时。独立复算脚本在ROOT外`research/thinlto-complete-result-recompute.py`；没有import冻结runner/visibility模块来替代独立算法，没有改变递归scripts集合。1192份run文件读前后hash一致。

## 身份、计数与完整执行

- run：`out/next-optimization/thin-complete-runtime-v1/`。
- protocol：`out/next-optimization/thin-complete-protocol-v1/protocol.json`，SHA `5b0ff104485ba18c34db170716ead50cc438bfc3b8c8ebc1c1d1266876946728`；run副本原字节一致。
- A：main `99035931d358add0eaed6d4535035c38413701dd`、tree `1c16744d838064cb1c687ba56ef3a0d17ed3130c`，binary SHA `64e1ff43bbaa59f786f1cc899b69e707072219a9c0abf0122ce31cd7b77f981d`。
- B：`2549cbec8e1f3096d4c8c8a960459c83095ad13d`、tree `ebb993128355fed8228627c6d7b37d5cb0a90d45`，binary SHA `fdfe6cea83286475582ce5b9063e9d84a9371b584baf99920193689f5e93d3e4`。
- R：真实toolchain Rust1.99.1，SHA `b53b780f77f952062f8ca8a3b70aef2390b1e45ad91939be94afdc9543026853`。

实际source/tree/binary/build-log绑定、全部脚本、工具和640份ZIL/Rust源hash已重新核对；LLVM实际版本为23.1.4，ld.lld argv0与Rust真实路径正确。6次clean编译全部exit0；54次native为12额外功能、6warmup、36formal，完整顺序与六排列一致。196份process-attempt均正常退出、reaped、无timeout/exception/残留group；没有把异常/缺格过滤成Pass。最终manifest是completed NoGo，runner退出1对应运行门失败。

## 当前预检与真实LLVM证据

| 检查 | 本次实际计数与核验 |
| --- | --- |
| 独立语言Verify | A/B共4请求均Verify1，stdout各有一次verify executed，完整source closure 64/256 |
| A当前unit | 16个builder：当前preflight、LLVM前后验证、完整default O2/after、x86-64/features空/PIC/default model |
| B当前machine | 2次原model、2次fold后分区输入、16次实际unit validate；外部preflight JSON与当前report中的receipt一致 |
| 实际prelink | 16份真实summary/HASH、当前input/prelink BC/dis文件与链接输入SHA逐项匹配 |
| 外部LLVM验证 | 16次原IR、16次prelink后、16次实际`.3.import.bc`、16次`.4.opt.bc`验证全部exit0；并保存16份`.3`实际dis文本 |
| 实际ThinLTO选择 | 相对原方案仅增一次force-import-all=true；O2/CGO2、PIC、CPU、ICF none、after merge/aliases false、总两CPU/两jobs保持；两次link真实imports各指向7个跨unit模块 |
| cache与失败诊断 | 两case均使用初始为空的独立native cache，保存值hash匹配；force stderr各只有已声明、无owner/body的getchar导入诊断，实际coverage仍独立通过 |

getchar的`Error importing module: Failed to import function getchar due to None`原行未删。它没有IR owner/body，由本协议限定的foreign白名单接受；不是忽略源函数导入失败。即使该诊断会中止一个caller扫描，以下对全部Required的实际定义核验仍成立，因此没有靠stderr白名单绕过覆盖门。

## 当前Rust成本与owner证明：独立从输入IR重算

本审用另一份whole-function文本抽取及固定点可达算法，独立于冻结visibility.py的逐行reader/DFS：从实际原input重算function的block+instruction成本及原跨owner闭包，再从实际prelink重新求Required，与当前Rust proof和最终`.3`逐项对照。全部8unit canonical集合唯一且不重叠，实际输入集合与proof完全相等；原input闭包恰等Allowed，没有用旧synthetic selfcheck冒认当前Rust能力。

| case | 原source cost | 原Allowed copy cost | unit0 canonical | 其余各unit canonical | unit0 Allowed定义数 | Allowed≤原预算 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| N64 | 35777 | 31360 | 68 | 72 | 504 | 是 |
| N256 | 144449 | 125440 | 284 | 288 | 2016 | 是 |

其余unit Allowed/copy cost均为0；unit0所有Allowed都由其他原canonical owner提供，没有复制自己、main wrapper或未知定义。各函数原cost、每unit小计和全局总计均与实际input文本一致。没有增加语言预算、复制预算或重设阈值。

## 实际Required、AE名单与费用

从实际prelink图的所有当前owned定义求闭包，unit0 Required分别为56和224，其余unit均为0。它们都来自原Allowed；本地prelink已经把helper内联进leaf value，所以无需把原Allowed中的每个helper符号再单独import。

| case / unit | Required | `.3`实际新增defs | 实际AE | Required覆盖 | 额外/越界defs |
| --- | ---: | ---: | ---: | --- | --- |
| N64 / 0 | 56 | 56 | 56 | 全部 | 0 |
| N256 / 0 | 224 | 224 | 224 | 全部 | 0 |
| 两case / 1…7各unit | 0 | 0 | 0 | 全部 | 0 |

实际集合满足`Required = ActualAdded = AE`，并且`AE⊆Allowed`、所有非AE也在own∪Allowed内。post-import调用目标没有越过原owner/Allowed边界。这里检查的是**真实definition及body**，不是declaration、imports文件存在或单个布尔标志；`.3`本身已实际LLVM verify。

完整实际AE名单可精确表达为：`zil.fn.(14+9*(i−1))`，其中`1≤i<N`且`i%8≠0`。N64为56项，N256为224项；每个i的owner为i%8。其他unit没有AE。已逐名从真实`.3`抽取并与该集合核为相等，包含例如`zil.fn.14/23/32/...`，没有按后缀猜canonical或略过未知clone。完整逐项名单、owner及费用见ROOT外[actual-audit.json](../thinlto-complete-visibility-actual-audit.json)，SHA `06ed83dc004869d5785c5f6b0b955d2b3d28a7cc59d70a9c0fef293f7a38f049`。

| case | 每个实际AE的原函数cost | 实际AE按原名字cost总计 | 原Allowed完整闭包cost | `.3`新增body的文本block+instruction数 |
| --- | ---: | ---: | ---: | ---: |
| N64 | 56 | 3136 | 31360 | 9699 |
| N256 | 56 | 12544 | 125440 | 38938 |

这些口径不能互换：AE的prelinked body已经包含本地helper展开，原名字cost只指原value函数；原复制预算证明使用包含helper的**整个Allowed闭包**31360/125440，未以较小的3136/12544偷减费用。最后一列是优化后实际LLVM文本统计，不是原machine预算或native动态指令/运行周期。每个实际新增body及其源owner都在既有预算覆盖范围内。

因此可确认的新事实是：**实际prelink需要的跨unit body已完整可见，运行质量仍显著退化。** 可见性成立不等于已经发生相同的内联、跨函数组合或非内联IPO，也不等于与main具有相同优化上下文。本审没有新增objdump/remarks分析，不把全部回退分配给某个pass；本结果否定这份单一force导入对照达到运行门，不是对所有后端增量架构的否定。

## 全部I/O与运行门复算

本审重新计算全部251个seed的四步动态整数递推，再按100000=398×251+102聚合，不调用generator oracle；全部value_by_seed、周期和三输入输出与protocol一致。54份真实stdin/stdout/stderr、program SHA、record/native-rows/process-attempt/raw-metrics逐项相符，全部exit0且正确。没有新运行program，也没有用编译器输出充当期望值。

每arm每case六formal全部纳入，六排列及round/position不变，未删长尾。wall/CPU为ms，RSS为KiB；小样本p95按冻结定义等于max，不是稳定尾延迟估计。

| case | arm | mean wall | median wall | max/p95 wall | mean CPU | RSS |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| N64 | A | 103.410099 | 102.750580 | 109.353406 | 102.670000 | 290976 |
| N64 | B | 167.328515 | 167.018070 | 176.429194 | 165.916333 | 290976 |
| N64 | R | 399.326689 | 402.564200 | 406.927108 | 396.118000 | 290976 |
| N256 | A | 315.316488 | 312.901490 | 332.986262 | 314.506167 | 290976 |
| N256 | B | 648.326222 | 642.334476 | 669.848701 | 644.949667 | 290976 |
| N256 | R | 1531.360022 | 1530.115772 | 1586.021933 | 1523.126667 | 290976 |

| case | B/A mean wall | R/B mean wall | 结果 |
| --- | ---: | ---: | --- |
| N64 | 1.6181061285（+61.8106%） | 2.3864832012 | 相对退化门失败，NoGo |
| N256 | 2.0561126545（+105.6113%） | 2.3620208028 | 相对退化门失败，NoGo |

Rust/B均高于0.90，但两门必须同时满足。RSS恒值存在launcher/fork高水位floor，不能推断真实live内存相同或省内存；总CPU0/1约束不等于锁钟频。不用环境猜测、均值裁剪或某一有利点豁免回退。

## 编译诊断计时及裁决边界

| case | arm | 完整编译wall ms | CPU ms | RSS KiB |
| --- | --- | ---: | ---: | ---: |
| N64 | A | 1387.960267 | 2411.149 | 290976 |
| N64 | B | 6485.986823 | 11270.618 | 290976 |
| N64 | R | 818.229374 | 1121.422 | 290976 |
| N256 | A | 4521.607675 | 7801.946 | 415024 |
| N256 | B | 8662.883235 | 14375.252 | 384324 |
| N256 | R | 2355.649615 | 3668.995 | 290976 |

B含当前原planner/proof、IR序列化、工具hash、实际dis/verify及完整ThinLTO/native link，未减掉这些开销；子阶段与完整请求嵌套，不能相加后扣除。本协议没有编译速度门，单次cold诊断不构成生产编译提速或净收益结论。

维持本次严格对照NoGo，不扩八case/compile/concurrency、不放宽原预算/门、不把“完整可见性证明通过”改称产品性能成功。旧default `thin-runtime-v1`无导入NoGo以原source/protocol/raw独立保留；两窗口没有直接随机化paired比较，不能用各自B/A差额宣称force带来微收益。main990及所有其他旧NoGo保持原身份。

| 证据 | SHA256 |
| --- | --- |
| protocol.json | 5b0ff104485ba18c34db170716ead50cc438bfc3b8c8ebc1c1d1266876946728 |
| native-rows.jsonl | 14b2540a43a0807d7da3c44c34a884b19fbcbf3cc5dd74b948cf852f669e15cb |
| summary.json | f113dce59c12dffb1ea89e031342ed3df5e626a03afd9908a741a1f05f7b1ee2 |
| execution-manifest.json | cf736886a28b5c5102f713409d2c768491340d94f07c74a0222d7f99a630ce23 |
