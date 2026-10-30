# CP22 逐模块解析复用：完整成本与后继修复

## 裁决

**CP22原候选NoGo。** 本轮实现了真实新进程间的逐模块解析复用，但原完整混合负载没有建立净提速；随后独立审核又找到临时路径碰撞可误删已发布可执行文件的正确性缺陷。初期测试、性能观测、晚期反例分别保留；不把后继CP26/CP27修复的效果回写给CP22。

候选源码 `f660aeb68e4cf86eab0bc7c9e219d5bcf8bdb5ea`，crates tree `a598b9f6b04feff21fc2eed24566726a1a79ef12`，交付 `7ca57fb61aa6fe4f7cac963a531a6cf2851792e4`。基线 `485301e1f1dcdfb0b4e24f6249355104243c3fe3`。GitHub历史候选对应 `74142bc822c57b1246c9bf328c019fd364ee150a`，全Git tree相同，提交元数据不同。

## 测量和观测

两臂同开source/object/query缓存，O0，8个编译单元/4个jobs，实际亲和CPU0/1。五语料权重5/15/30/30/20，四请求权重10/35/40/15；3个固定ABBA/BAAB配对区块，seed20261029。每个请求均为新CLI进程，外部wall/CPU包含发现、读取、缓存、Verify、LLVM、链接和发布；OS页缓存未冲刷，不是长驻会话试验。

正式240/240请求完成，768次计时外原生校验通过；预检40/40和128次原生校验单列。每个相同语料快照的ELF散列一致。独审逐行复算配对均值、区块中位、绝对成本加权后的比值，未对百分比直接取平均。

| 请求 | 基线wall秒 | CP22 wall秒 | wall B/A | CPU B/A |
|---|---:|---:|---:|---:|
| 空编译器缓存的首次请求 | 1.326700 | 1.764387 | 1.329906421 | 1.139285432 |
| 未改动的新进程请求 | 0.703930 | 0.695630 | 0.988208008 | 0.986340073 |
| 私有实现编辑 | 0.958340 | 0.894081 | 0.932947500 | 0.938659858 |
| 公开接口编辑 | 0.915595 | 0.937571 | 1.024001495 | 1.018121688 |
| 预先固定的混合负载 | 0.899720874 | 0.918177027 | 1.020513198 | 0.991748631 |

wall的探索性配对区块bootstrap95%区间为 [0.964195370, 1.081159068]。共享宿主、3个区块；不能把这个描述区间当产品验收或统计上稳定提速。

私有编辑wall点估计降低约6.7%，首次建缓存增加约33.0%，混合wall增加约2.05%；不是可采用的整体性能胜出。最大语料首次请求比为1.451909344，私有编辑0.911351923，未经改动0.946303911；其余全部20个case/request单元保留在原summary，不只报告获益格。

## 实际复用是否发生

120条候选正式记录全部与当前模块身份/源码字节历史相符。五语料模块数分别7、35、259、1027、4。每个四请求序列中：首次解析N/存储N，未改动命中N/解析0；私有编辑命中N−1/解析1/存储1；公开编辑命中N−2/解析2/存储2。没有quota_miss、corrupt、bypass或write_error；检查点状态为miss/hit/miss/miss，每次独立Verify仍为1。

这证明缓存路径真实参与了请求，不能据此推导Expand、CT、Types已经实现函数级增量。那些阶段仍按完整项目身份执行；解析收据只取消与当前剩余token/node额度无关的身份污染，并按原顺序回放实际费用，不能跳过当前低预算首错。

## 缺陷、成本假设与后继

晚期独审在冻结CP22 CLI上把输出 `-o` 设为解析缓存将使用的确定性临时路径。关闭缓存时ELF存在并exit0；开启缓存时仍报Success、Verify1，缓存写入AlreadyExists，却因预先构造的Temporary guard删除了并非它创建的ELF。最终缓存路径的输入/输出重叠检查没有覆盖临时路径。详见CP22-review-addendum与完整碰撞归档。

同类审查在原485基线的CP12检查点、LLVM对象缓存分别复现独立问题，不能说是本片引入；raw ArtifactCache的open后arm顺序没有这个问题。CP26只修解析发布，CP27只修上述两处既有所有权。

静态成本审查发现原解析冷发布每模块有多次write、sync_all和rename，成功后还尝试unlink；120条正式记录及wall/CPU差异与磁盘发布费用假设一致，但还不能据此证明因果。CP26在既有“advisory缓存不承诺断电耐久、损坏可重建”合同内缓冲完整写入，flush/close后rename，同时修复临时文件所有权。它必须单独与CP22及原基线计量，禁止乘接跨窗口比率。

## 可复核证据

- [原始完整窗口与内置逐文件清单](../../../evidence/research/compile-performance/COMPILE6/cp22-o0-query.tar.xz)，SHA256 `2416a94a0133dfaa26dc1fa6fd23aca25f91bb52f0544896814cd99c34c7d36d`；10414个文件，63363022字节原始内容，每个tar条目解码后逐字与源核对。
- [完整summary](../../../evidence/research/compile-performance/COMPILE6/cp22-o0-query-summary.json)、[独立数学/原始行复核](../../../evidence/research/compile-performance/COMPILE6/CP22-full-cli-review.json)、[实际解析命中审核](../../../evidence/research/compile-performance/COMPILE6/CP22-parse-engagement-review.json)。
- [初期独审](../../../evidence/research/compile-performance/COMPILE6/CP22-review.json)与[晚期ChangesRequested补充](../../../evidence/research/compile-performance/COMPILE6/CP22-review-addendum.json)均保留；初期106项作者测试、10项非作者复跑、94组API与60次CLI控制不能相加成互异测试数。
- [三处真实碰撞脚本、stdout、产物和收据](../../../evidence/research/compile-performance/COMPILE6/temp-ownership-findings.tar.xz)。
- [ZIT完整机制分析](ZIT-QUERY-REVIEW.md)、[CP26/CP27派工](COMPILE6-PLAN.md)。

不改变main、不改产品profile；完整语言NotSatisfied、产品NotRun、父任务B0 Doing保持。
