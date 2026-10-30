# CP23 查询读取快路径：完整成本 NoGo

## 结果与身份

**生产采用NoGo。** 缺失查询与无依赖叶查询的快路径保持了受测正确性，但完整新进程CLI窗口没有建立净收益。非叶闭包检查与active-cycle优先级原样保留。微测、正确性控制、首次失败窗口与恢复窗口分别保存。

候选源码 `4496b1ab6c62ff1131dfbe4e09b33467f346099c`，crates tree `96d1fd21ba1e1ccbd1b9418d7a62c555875445d6`，release SHA256 `627ebcf0757b5ba17e8ff37eca3f0c1653246c74cb313fad5f1011a39822564c`；基线 `485301e1f1dcdfb0b4e24f6249355104243c3fe3`。GitHub独立候选 `1cb27ac6b907aeef28b8a472d15ee1859b9959c1` 与本地源码完整Git tree相同，提交元数据不同。主研究树未采用此生产diff。

恢复窗口使用原五语料权重5/15/30/30/20、四请求权重10/35/40/15，三ABBA/BAAB区块，seed20261029，O0、8units/4jobs、CPU0/1。对象缓存开启，query cache关闭。两个窗口的source/binary/tool/fixtures/weights/order/env保持相同；新计划仅创建时间及runner所在文档提交不同。这里的旧新ZIL比值不能与query-on窗口混算。

恢复预检40/40、128次原生对照通过；正式240/240、768次计时外原生对照通过，每个相同语料快照ELF hash一致。每次请求是新进程，计入发现、读取、编译、独立Verify、LLVM、链接与发布，OS页缓存未冲刷。

| 请求 | 基线wall秒 | CP23 wall秒 | wall B/A | CPU B/A |
|---|---:|---:|---:|---:|
| 首次请求 | 1.276934326 | 1.259382933 | 0.986255054 | 1.002981293 |
| 未改动 | 0.774416871 | 0.806590048 | 1.041545036 | 1.050908286 |
| 私有编辑 | 0.818593589 | 0.868618249 | 1.061110495 | 1.066058646 |
| 公开编辑 | 0.904310726 | 0.863108930 | 0.954438452 | 0.959544903 |
| 固定混合负载 | 0.861823382 | 0.885158449 | 1.027076391 | 1.035669598 |

wall点估计增加约2.71%，CPU增加约3.57%；探索性配对区块bootstrap95%为 [0.975744003, 1.052894151]，跨过1。共享宿主与三个区块的范围内未证明稳定收益，也不把这个结果当作严格因果上的退化证明。

## 全部语料/请求格

| 语料/请求 | 基线wall秒 | CP23 wall秒 | B/A |
|---|---:|---:|---:|
| tree4x2/cold_process | 0.408665163 | 0.397236783 | 0.972034854 |
| tree4x2/unchanged | 0.377557957 | 0.399398686 | 1.057847352 |
| tree4x2/private_edit | 0.361375195 | 0.407465056 | 1.127540188 |
| tree4x2/public_edit | 0.346671823 | 0.447650981 | 1.291281698 |
| tree32x8/cold_process | 0.468014668 | 0.415107892 | 0.886954876 |
| tree32x8/unchanged | 0.396100752 | 0.395918186 | 0.999539092 |
| tree32x8/private_edit | 0.378746879 | 0.437719958 | 1.155705781 |
| tree32x8/public_edit | 0.375262315 | 0.439263126 | 1.170549530 |
| tree256x8/cold_process | 0.982964068 | 0.912135174 | 0.927943557 |
| tree256x8/unchanged | 0.600313019 | 0.602064568 | 1.002917725 |
| tree256x8/private_edit | 0.645571626 | 0.633333756 | 0.981043358 |
| tree256x8/public_edit | 0.676928103 | 0.690743252 | 1.020408593 |
| tree1024x8/cold_process | 2.729662744 | 2.773220326 | 1.015957130 |
| tree1024x8/unchanged | 1.498512590 | 1.576708755 | 1.052182521 |
| tree1024x8/private_edit | 1.611823759 | 1.747900986 | 1.084424383 |
| tree1024x8/public_edit | 1.860485967 | 1.647563840 | 0.885555639 |
| retained/cold_process | 0.362554118 | 0.358241300 | 0.988104346 |
| retained/unchanged | 0.332380886 | 0.368001945 | 1.107169393 |
| retained/private_edit | 0.332470910 | 0.341082899 | 1.025902985 |
| retained/public_edit | 0.347317834 | 0.366723919 | 1.055874141 |

## 原失败窗口不撤销

原 `cp23-o0` 首轮preflight在实际ENOSPC后中断，保留原截断records和日志；其恢复预检通过后，正式240行中的索引57、58（第58、59行）出现LLD/Clang异常，driver返回ZILInfraError/E-LLVM-COMPILE，原summary为IncompleteOrFailed_NoScore。错误文本包含Bus error，第一处堆栈涉及lld InputSection::writeTo。

同一时期确实观察到存储压力与已删除临时文件被环境重新物化，但没有证明它就是两次链接失败的原因。缓存块失效回收后新窗口未复现，只能记录本次全部成功，不能把旧错误归因升级为定论，也不能挑出旧238条拼进新240条。

## 正确性、微测与适用限制

候选12项定向数据库测试及9600个DAG/指纹/驱逐oracle由作者和非作者分别核对，重叠计数不相加。96行微测中的缺失/叶读取收益和部分控制负结果均保留在候选证据；它们没有覆盖完整编译成本。

当前普通CLI每次创建新Session，阶段图仍是多模块Parse汇入少数整项目查询；这个数据库读分支并未减少Types、Analyze、Verify或代码生成的实际工作。不能把局部少分配直接解释为完整增量编译突破，也不能把当前完整成本NoGo解释成zit点对点机制没有价值。

## 证据

- [完整恢复summary](../../../evidence/research/compile-performance/COMPILE6/cp23-o0-recovered-summary.json)。
- [恢复窗口完整原始归档](../../../evidence/research/compile-performance/COMPILE6/cp23-o0-recovered.tar.xz)；plan、fixtures、全部stdout/stderr、预检、正式行、收据与内置逐文件清单一并保存。
- [原失败窗口完整归档](../../../evidence/research/compile-performance/COMPILE6/cp23-o0-failed.tar.xz)，SHA256 `9b5b4834e0ba46f64a7f27283c650121cc4f9ebcba00f5e772b29c4529f6b5f4`。
- [独立完整窗口复核](../../../evidence/research/compile-performance/COMPILE6/CP23-recovered-full-cli-review.json)。
- [原始恢复计量入口收据](../../../evidence/research/compile-performance/COMPILE6/CP23-recovered-measure-orchestration.json)。

完整语言NotSatisfied、产品NotRun、父任务B0 Doing与main采用Hold不变。
