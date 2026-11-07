# 第六阶段结果：增量边界、O2 差距与候选取舍

本轮已经把旧图算法、修复适配后的 zit、生产 DB、跨进程语义候选、标量发射、ThinLTO、MergeFunctions 和 SLP 分别实施或实际消融。结果支持继续减少真实前端工作、改善机器输入，并攻克已定位的 LLVM 热点；同时淘汰了没有净收益的反向 DB 和灾难性退化的 early/closed 合并组合。当前证据尚不能宣布通用编译速度突破、默认 O2 全面追平 Rust，或运行质量已经全面相当。

本页是**已冻结结果稿**，不是最终组合交付验收。专项已以 `ac4ebb9e5…` 合入 main 的 `f2572502…` 更新，并以 `888af53a0…` 补齐撤回 DB 后的配套测试；方向是合入专项分支，不是把专项合入 main。整合后的限定正确性身份已冻结，最终 LLVM 工具链组合和相应完整矩阵仍将另行绑定。下面每个数字仍属于原来的源码、工具、ELF、输入和窗口，不能重标为合并后的新二进制，也不能相乘成为“累计提速”。README 入口暂不切换。测量定义见 [STAGE6-PROTOCOL.md](STAGE6-PROTOCOL.md)。

## 主分支整合与DB配套测试修复

`ac4ebb9e5fe668b160cbc4b1fe30db735858fdce`保留上游f257的规则与实现，只在Expand冲突文件作手工整合。root限定检查通过75项测试、七个指定crate的llvm-api/all-targets Clippy与workspace fmt；这不是全workspace测试验收。独立复核在精确ac4源码上增加3项组合测试，比较29对完整warm/fresh报告，并完成4次LLVM API O2原生执行。拒绝后的restore可能重新Parse多份文件，不能把成功编辑的“一份Parse”保证推广到失败epoch；四个初始审计夹具/假设失败及修正均保留。

GitHub运行`41206406105`真实失败：此前撤回反向DB实现时没有同时恢复其私有单元测试，遗留测试仍调用已撤回API。原75项及独审3项没有编译zil-db的`cfg(test)`，未覆盖这一问题，不能将旧限定Pass重新解释为完整CI通过。`888af53a0a7389d10a508250059db6b631c61e4a`将DB测试逐字恢复到3d6配套版本，保持生产DB不变；default/reference各13项（13个唯一测试、26次执行）及DB Clippy通过。补审核对源码和root日志，明确独立重跑次数为0，没有把日志检查算作新测试。

ac4与888的release CLI逐字相同，SHA256均为`0d0e05f4b3ba14c298fef7907c8a6ec23f40f01351509c90afea63c1289246e1`；这个事实不把旧ffe4性能窗升级为新整合结果。新benchmark仍须单独绑定实际二进制和LLVM库。归档收件时root报告后续`412072…` CI仍进行中，其未冻结记录不在本包，也不计Pass。静态merge材料、失败CI原log、DB修复前测试以及清理旧Rust缓存的逐文件身份和中断/恢复收据见`integration-correctness`包；工作树、target与仍活跃的API/plugin/runtime/SSA研究未打包。

随后取得的磁盘恢复证据没有把CI失败隐去：`41207249551`在888上因runner日志写入ENOSPC失败，下载的failed.log为空、logs.zip仅22字节且无条目，不能从这些空日志推定源码测试结果。root又发现初版清理步骤由sudo创建父目录造成后续普通用户不可写，以及umask导致root收据可能不可读；最终改为普通用户先创建父目录，并仅将该JSON显式设为0644。原作者/独审的同用户控制不覆盖sudo跨UID，两次修订与控制失败均原样归档。`41209327141`的真实预构建清理收据已上传，空闲空间从11624448000增至33555800064字节（10.826→31.251GiB）；这只证明清理和收据上传成功。收件时整次CI仍进行中，不计全CI Pass。见`ci-disk`包。

后续正式GitHub结果另存于`ci-main-412093`：运行[41209327141](https://github.com/zil-lang/zil/actions/runs/41209327141)最终为completed/success，精确HEAD为`2e8f8c1f6e4bcb92d829cfa91cc4582c230f1e3e`，完成时间2026-10-31 14:39:16 UTC。原先“进行中”的收据和失败包保持不变；成功不属于后来74431组合LLVM、6ed研究workflow或当前HEAD，也不替代那些候选自己的CI、运行质量和性能矩阵。完整run/jobs/artifact-list及原日志ZIP已保留。

## 已经作出的取舍

| 路线 | 本轮实际进展 | 已冻结结果与当前边界 |
|---|---|---|
| 旧076局部域/图方法 | 适配当前前端，重跑26族；直接、intern、BDD fused/cut均有真实尝试 | 16资源形状的局部直接查询核约1.90倍速度；分支存在回退，大根函数仍受表示/枚举预算限制。收益不能归给BDD切割或完整编译 |
| 事件支持因子化 | 从真实TypedIR转移直接求解，保留完整sum-of-products联合关系 | 能保留128个独立bool的入口乘积；完整资源查询最坏回退6.540倍。容量扩展成立，通用速度优势不成立 |
| zit适配与生产反向DB | 修复精确环检测、拓扑秩、失败恢复；生产接入反向边和相等屏障后做隔离消融 | JS适配开销明显增加；生产DB六区组未证明净收益，默认DB已完整恢复3d6原实现 |
| 真正减少前端工作 | 保留初始化纯投影；新增可信本地原始TypedProgram缓存；ffe4保留项目原始Parse Arc | 前两者分别是有限子分析复用和整项目精确候选复用；ffe4实际复用未改模块的parser结果。均不等于函数级Types增量 |
| 显式O2分区/正文导入 | 固定owner块、唯一canonical定义、受限优化副本及真实对象键；已完成六族运行质量核验 | 原策略运行性能GM退化5.393/9.582倍，8192算术约326倍；已NoGo，不按同等级O2质量晋升。新修复单独测量 |
| 标量局部提升 | 在有限白名单和两层可证明条件下消除不可观察栈往返 | ca79两区组有局部改善；默认19/40、closed17/40时间单元不利。没有消除主要LLVM热点 |
| ThinLocal | 真正CGU、pre-link bitcode、真实lld后键、缓存损坏恢复和section GC | 完整服务时间几何比约1.115/1.104，默认与closed总体回退；不晋升默认 |
| MergeFunctions | 显式off/after/before-and-after，地址/导出控制 | late有局部收益；early+closed在8192函数出现约100秒对约2秒的灾难性退化，停止并淘汰该组合 |
| SLP与LLVM定位 | 纠正无效setter消融，证明实际SLP gate，采集真实LLVM调用栈 | SLP不是两个opaque大形状的主瓶颈；JumpThreading重算支配树、InstCombine各有具体热点。plugin正式完整矩阵待测 |

## 样本覆盖与计量口径

生成器保留23个形状：单文件128—8192函数，4096函数在8/64/512/1024模块间均分或偏斜，8192函数的4×2048、8×1024及偏斜布局，长体、分支、类型、泛型、CT、链和扇出。图关系另有资源数量、条件数量、循环、拥有闭包及相关性反例。大模块分布比旧小例充分，但仍不是大型工业多crate工程的代表性抽样；常量输入的标量helper树也不能代表opaque分支/算术优化压力。

529的23族源码功能窗口是230请求：220有效，10个既有预算拒绝；有效请求为ZIL105、Rust115。`body-64x16`、`types-64x16`的ZIL请求在到达Verify前受限，失败没有删掉或通过提高预算变成成功。21个接受族中，unchanged/restore有42次原始语义候选命中，cold/两种编辑有63次missing。它验证了输出和缓存路径，不提供正式速度结论。各性能初筛实际选择的8/9族与Session的7族在原manifest中明确列出，不能把“工具支持23族”写成“每个候选都测完23族”。

| 观测 | 时间包含什么 | 内存及解释边界 |
|---|---|---|
| 完整CLI初筛 | 进程启动、当前输入/身份、缓存I/O、实际前后端和链接；生成源码/native在外 | `wait4`报告的峰值RSS，不是所有进程驻留之和 |
| 六区组Session | 内存中的完整`compile_project`调用；源码写入、fresh/native控制在外 | native断言之后的父进程当前VmRSS，不是请求峰值或链接器峰值 |
| 图/因子影子进程 | Parse/Expand/Types、关系候选、相应oracle和输出物化 | 不执行计时中的Analyze/Verify/Lower/LLVM/link；生产Analyze/Verify只在prepare控制 |
| Thin full-service | 编译器/Clang/LLD身份审计、当前Verify/preflight、序列化、优化、缓存及完整链接 | 每请求工具全文SHA约0.5秒级开销也计入，不能换成较小的driver-only时间 |
| pass表与perf | 插桩阶段CPU/墙钟或单次采样调用栈 | 属功能/诊断窗；不能当排他端到端、运行吞吐或调用次数测量 |

两区组只用于筛选，不补四组后追认为新的六区组确认。六个AB/BA交替双臂区组是每单元十二个观测。Session主比值为各臂六个值的中位数之比；图局部/稠密亦为臂中位数之比；zit和因子报告的是六个配对比值的中位数。不能混用这些统计量。失败、ResourceLimit、Unsupported、中断及NotRun均保留；完整比较合同不成立的单元没有速度比。

每个正常成功生产请求保持当前独立语言Verify；缓存命中不恢复Verified权限。损坏原始语义候选被Verify拒绝再从源恢复时，实际两次Verify会如实报告。机器预检、LLVM验证、语言Verify各有职责，不能互称替代证明。

## 旧图方法：重新测出了局部收益，也继续暴露表示边界

研究crate保留旧076/071算法及独立原始指令oracle，适配当前schema，未知类型/常量明确Unsupported。26族准备后，14族同时通过当前生产接受性及完整查询合同，进入六区组414个新进程。53项历史正反例通过。方法中确实重试了dense/local fused BDD和dense cut，未用直接转移的收益冒充BDD切割收益。

| 真实形状 | 局部direct/稠密direct查询核 | 完整研究进程 |
|---|---:|---:|
| 顺序资源1 | 1.160 | 0.931 |
| 顺序资源8 | 0.689 | 0.675 |
| 顺序资源16 | 0.526 | 0.610 |
| 两个独立条件 | 1.543 | 1.155 |
| 4096函数、64模块均分 | 0.991 | 0.972 |
| 长体16×16 | 1.014 | 1.013 |

旧小分布的NoGo不能直接外推：16资源的局部直接核有约1.90倍速度。但512—8192函数根、24/32/64资源、6/8条件仍遇旧槽/位宽、完整行集或oracle预算；顺序资源本身并非指数路径，不能把所有失败都解释为“输出必然指数”。large128的direct完整成功而BDD候选根函数ResourceLimit；types32的1024个新类型函数Unsupported；guarded-desc被当前生产Verify拒绝。helper成功不代替根函数完成。

进一步实现的`cut-support`按真实事件表达式读写支持分量直接转移，以初始BDD等式检查乘积分解，保留同一原始prestate和完整故障/退出关系；同名故障用备选乘积的并集处理。最初巨型guard OR导致资源8/16预算失败，修复后的sum-of-products继续完成控制，旧失败记录保留。

17族功能控制后，正式120个进程比较8个小族×6区组×2臂，并单独测4个大bool族×6压缩查询。小族均完整展开并对独立oracle；对更强的local-direct基线，资源1/4/8/16的因子进程比为**1.148/1.977/3.754/6.540**，小bool接近或稍快，不能无条件替换原求解器。

16/32/64/128个独立bool参数分别保留17/33/65/129个入口因子，128参数入口可表示`2^128`种联合赋值。压缩进程中位为1.744/2.681/5.627/17.512ms，但这些函数为`ExpansionDeferred`：没有运行完整独立oracle，没有完整物化，canonical fingerprint及对失败枚举的速度比均为空。结构条件和800个成员正例、8反例、255单字段投影是有限控制，容量结果不等于完整编译加速。

消费端又独立重建779+464个完整因子关系，行集一致；实际事件支持子集全部单行，field-cell没有减少。另一组field-cell虽减少，原packed u64表示仅17,803个cell，不能把计数变化称为内存压缩。完整来源、失败、预算和统计见 [STAGE6-ALGORITHMS.md](STAGE6-ALGORITHMS.md) 与 algorithms包。

## zit、生产DB与真正的增量缺口

适配实际实现了精确环检测、可升降拓扑秩、实际消费者重排、稳定边、相等屏障和单查询失败回滚，修复了应用到编译场景的问题，未把原JS内核的用途约束当成其自身bug。5000次合法换向、1200层链、真实环和随机DAG控制均完成。

七个输入取自59d真实LLVM调用拓扑，但节点计算是数值传播控制。六区组84臂中，适配/原版冷初始化比1.75—3.33，编辑比1.99—4.35，均有明显开销。8192函数图两版每次编辑都只重算4查询，适配仍读取8199次、快照8198条边、做24594次形状检查。不能用4对8195节点推导编译器千倍速度；不同失败恢复合同也没有成本比。

随后把反向依赖、按指纹种类失效及相等屏障真实接入生产DB，并用EE与仅恢复旧DB的精确EE快照做六区组隔离。两臂都保留初始化投影复用。840请求/70单元完整，时间37/70不利；check/O0等权单元几何比为**1.009864/0.998980**，没有证明净加速。

VmRSS有52/70单元不利，中位单元差+130KiB，但全体几何比0.997578；large2048/O0五个单元减少约16—20MiB。因此结论是没有足够净益支持额外机制，不能声称每个样本都更慢或内存普遍增加。默认DB已完整恢复3d6原字节，EE候选、修复控制和反向结果仍保存。

更重要的逻辑问题在查询边界：当前driver将完整内容版本放进QueryKey；编辑发布的是新owner键，而反向索引从同一个旧键的消费者出发，二者不会自动接起来。unchanged直接复用raw对象，也不会通过重新运行producer产生新的语义相等屏障。不能简单删掉键里的源码hash救场，因为当前命中路径不会另核新的依赖/观察，可能直接返回陈旧raw数据。稳定owner、当前有效性、实际依赖及事务更新必须分开设计；FileId/ModuleId变化、查找不存在性、泛型/CT正文、规则、target及预算仍要成为有效性证据。

实际初始化投影只复用纯局部CFG/init子问题及其原逻辑费用；其他Analyze和当前Verify照常。其边界比整函数、整项目或跨进程增量窄。完整推理和逐键事实见 [research-reactive-key-attribution.md](../../../changes/research-reactive-key-attribution.md)。

ca79对3d6的另一个六区组Session组合窗也完成840请求/70单元：时间17/70、VmRSS47/70不利。O0冷/暖几何比0.9361/0.9281；even1024x4/O0冷805.513→674.206ms，探索性区间[0.697,1.050]跨1，同时该族check冷374.048→409.699ms不利。large2048/O0私有编辑233.636→210.716ms伴随VmRSS134056→148964KiB。组合改善不能归给已撤回DB；区间未做多重比较校正，全部140单元见Session独审表。

## 原始语义候选与项目语法保留，各自只解决一层工作

529的显式`--semantic-cache`保存可信本地的**原始TypedProgram候选**，绑定完整源图、路径/ID、规则、全部limits、目标、build inputs及编译器身份。命中仍做当前Analyze、独立Program Verify、接口/入口及后端/产物检查。完整成功后才发布；损坏按miss，候选拒绝按实际次数从源码恢复，Verify之后的后端错误不重复外部动作。它不保存Verified能力，不认证任意外部写入者，也不是私有编辑后的函数级Types增量。

独审完成完整TypedProgram roundtrip、真实CLI及26条身份/预算控制。23族功能窗口表明unchanged/restore能命中而两类编辑仍missing；完整CLI最终收益必须把身份、读写、解码、分析、验证、链接全算进去，不能只报省去的前端时间。

另一个已定位的实质浪费是项目语法的所有权：32个raw根中四个整项目阶段占四项，大项目只剩28份Parse。1025文件的私有编辑因而执行997次Parse。ffe4让当前ExpandedProject持有原始Parse Arc索引，并在真实filesystem loader计算路径复用，而非只修改事件。控制中实际只解析变更的一份文件，仍逐文件读取当前字节、核导入/路径/ID/规则和原费用；旧Core/Typed会释放。

该实现的作者与独审正确性控制已冻结，包含初始预算夹具失败和修正。新增AST常驻字节是实际成本：raw根数量仍32不表示内存字节有界。后续Expand/CT/Types仍是粗粒度工作，不能把parser命中率当整编译提速。

新的两个六区组Session窗口已经完成，并由非作者逐请求复算：`ccf1→ffe4`隔离项目语法持有；`3d6→ffe4`测这一旧身份下的组合。每窗840请求、70单元、420次native输出和126条准备控制；共438620份实际源码逐文件核SHA，1680次当前Verify均恰好1，错误/取消/N−1拒绝及恢复控制完整通过。源码收据分别绑定638/639份构建输入；测量工具、probe ELF、直接rlib及CLI身份均核对，没有用后来的main工作树替换原身份。

以下为各请求类别七个分布的时间比几何均值（候选/基线）；每格基础值仍是六次臂中位数之比。全部140单元的绝对时间、当前VmRSS、固定2000次配对区组bootstrap探索性95%区间和所有回退见`syntax-session-review/recomputed.json`及`review.md`，未做多重比较校正。

| 对照/模式 | cold | unchanged | private | public | restore |
|---|---:|---:|---:|---:|---:|
| 语法隔离 check | 1.006652 | 1.007627 | 0.943220 | 0.965673 | 0.947058 |
| 语法隔离 O0 | 0.979436 | 1.008205 | 0.956661 | 0.960590 | 0.962522 |
| 对3d6组合 check | 0.968908 | 0.972305 | 0.925949 | 0.924691 | 0.967401 |
| 对3d6组合 O0 | 0.978291 | 0.970544 | 0.937338 | 0.944833 | 0.964878 |

语法隔离时间23/70、VmRSS22/70单元不利；对3d6组合时间15/70、VmRSS37/70不利。隔离的even1024x4/check/public为365.107→313.342ms（0.8582，探索性区间[0.7192,0.9987]），但同族check冷379.409→422.802ms（1.1144，[0.9744,1.6523]）回退，O0/public也为1.0309。组合的large2048/O0/restore为100.116→108.837ms（1.0871，[1.0268,1.1312]），当前VmRSS184200→195508KiB。不能只报告编辑加速，或凭未经多重校正的个别区间宣布普遍显著。

事件级控制确认：1025源码的局部编辑在两窗全部相关请求上从997次Parse降到1，后续整项目Expand/CT/Types/Analyze仍执行；unchanged原有粗粒度命中照常保留，Verify每次当前执行。这个改动确实减少工作，六区组完整请求收益为有限幅度，并未形成数量级突破。上述两窗仅为ffe4的check/O0；合入f257和新LLVM库之后的生产CLI/Rust/运行质量仍需用各自新身份验证。

## Rust对照暴露的O2差距与显式分区初筛

实际对照Rust1.99.1 commit `58cf8f9c…`使用LLVM23.1.1；ZIL为官方LLVM23.1.4，具体源码/工具SHA保留。源码和工作产品审计纠正了一个关键归因：`codegen-units=8`是上限，不保证一个大源码module被分成八块。large8192每代Rust O2实际只有三个工作对象，主CGU预LTO bitcode仅3436B、对象1608B，普通helper已在预优化阶段消去。8192函数均分本身解释不了Rust优势。

Rust的稳定/volatile组、按内联重叠合并、私有符号内部化、ThinLocal和真正pre/post-LTO keys分别有具体作用。增量构建还关闭部分按正文大小的自动跨crate内联推断；不能只看到“Rust内联”就盲目增加复制。ZIL默认外部导出合同与Rust私有函数可删除性有差别，显式closed必须独立比较。

EE的显式`--incremental-o2`用固定256函数owner块，唯一canonical定义保留符号，其他CGU收到有界available_externally优化副本；实际正文/声明进入机器输入和对象键。它是受控候选，不是Rust分区的复刻，也不同于旧076资源联合关系图方法。导入上限可能保留单体O2本会内联的调用，因此LLVM仍叫O2不保证运行质量相同。

EE三窗共540请求/135单元，每窗两区组。8192单文件的完整请求中位比为：

| 对照 | cold | unchanged | early edit | late edit | restore |
|---|---:|---:|---:|---:|---:|
| EE显式策略 / Rust，默认导出 | 1.748 | 1.856 | 2.556 | 0.771 | 0.730 |
| EE显式策略 / Rust，closed | 1.477 | 1.622 | 2.513 | 0.829 | 0.738 |
| EE显式策略 / 3d6，默认隔离 | 0.677 | 1.028 | 0.672 | 0.202 | 1.020 |

这里同时有后期编辑优势、早期编辑仍超过Rust两倍、暖/恢复相对旧ZIL回退。隔离冷构建峰值RSS为410110对377998KiB，unchanged为345356对296962KiB。不能只展示0.202，也不能把这些两区组观察替代最终CLI/Rust/opaque运行质量矩阵。完整45单元/窗和所有分布保存在screens包。

## ThinLocal与MergeFunctions：借鉴后实际筛掉不合算的策略

Thin工具使用真正结构化CGU而非公开`program.ll`代替单位输入；当前ValidateModule和语言Verify保留，pre键绑定完整文本/target/工具/flags，post键由真实lld ThinLTO生成。所有成功请求仍链接。初次漏section GC、错误lld调用别名、环境缺失和断言错误分别留档；修正前的结果没有混入后来的两臂对照。

加入section GC确实将一例closed的global函数数512→0、exe32160→5968字节，默认导出根仍保留，但代码小不等于编译快。两个主窗360请求中有40个body64预算拒绝，另补40个body16请求，合计**400请求=360有效+40拒绝**。有效45个case/request单元的full-service Thin/Clang几何比为默认1.114656、closed1.104023。

| 8192请求 | 默认Thin/Clang | closed Thin/Clang |
|---|---:|---:|
| cold | 1.286 | 1.349 |
| unchanged | 0.969 | 1.024 |
| early edit | 1.347 | 1.390 |
| late edit | 2.118 | 2.177 |
| restore | 0.988 | 1.127 |

closed首区组text从247567降到1143字节、exe从510184降到6432字节，仍不能掩盖编译回退。此次不晋升Thin默认；后缓存腐坏控制也补测了当前真正需要的key重生成，不能用历史无关key的拒绝冒充回补。

Rust O2启用MergeFunctions的事实则推动了另一项实际候选。43037同一编译器内比较off/after/before-and-after，保持地址、导出与缓存身份。三个完整160请求窗口中，default-after、default-before-and-after、closed-after分别有12/40、9/40、17/40时间单元不利。8192默认late策略五请求比为0.814/0.904/0.825/0.867/0.963，有局部收益；closed late则为0.972/1.015/1.012/0.973/1.061。

第四个early+closed窗在首区组large8192完成cold100.609秒、early98.733秒及late约100秒，而off/closed约2秒。负责人观察到灾难性退化后停止：**55有效、1人工中断、104 NotRun**；总矩阵实际535有效而非640完成。中断耗时为空，不生成完整单元比值；STOP.json和退出−15保留。该组合不再晋升，未将停止阈值追认为事先规定。late的局部改善仍需运行质量与最终组合验证。

## Scalar：减少不必要机器工作，但没有解决全部LLVM成本

ca79在有限Bool/U8/U16/I32/I64形状上，通过源块条件及机器槽用途二次核对，去除不可观察的alloca/load/store。地址暴露、资源/捕获/投影及不能证明的形状回退；不重排调用、trap、清理，不引入跨源块Phi。正确性独审与真实O0/O2/native控制通过，不能据此推导运行吞吐。

两区组隔离对照为ca79对43037，两臂都用MergeFunctions=after、SLP开启，不用语义缓存或正文导入；320请求全有效。以下列出全部80时间单元，数值为候选/基线的臂中位数比，四舍五入为1.000不代表严格相等。默认19/40、closed17/40不利。

| 合同/形状 | cold | unchanged | early | late | restore |
|---|---:|---:|---:|---:|---:|
| 默认 large128 | 1.005 | 1.032 | 0.981 | 0.960 | 0.740 |
| 默认 large2048 | 0.943 | 0.834 | 0.919 | 0.975 | 1.015 |
| 默认 large8192 | 0.894 | 1.064 | 0.924 | 0.827 | 0.988 |
| 默认 branch64×16 | 0.944 | 0.901 | 0.986 | 0.966 | 0.965 |
| 默认 body16×16 | 0.955 | 1.011 | 0.975 | 1.017 | 0.962 |
| 默认 even4×2048 | 0.962 | 1.083 | 1.029 | 1.044 | 1.064 |
| 默认 skew8 | 1.031 | 1.038 | 1.017 | 1.006 | 1.062 |
| 默认 types32×16 | 1.020 | 1.006 | 0.947 | 1.016 | 1.025 |
| closed large128 | 0.929 | 0.975 | 0.978 | 1.002 | 1.144 |
| closed large2048 | 0.946 | 0.866 | 0.901 | 0.972 | 0.977 |
| closed large8192 | 0.966 | 1.030 | 0.955 | 0.940 | 1.027 |
| closed branch64×16 | 0.962 | 0.994 | 1.000 | 1.040 | 0.954 |
| closed body16×16 | 0.973 | 1.010 | 0.905 | 0.956 | 1.020 |
| closed even4×2048 | 0.995 | 1.084 | 1.057 | 1.060 | 1.039 |
| closed skew8 | 0.935 | 1.082 | 0.995 | 1.031 | 1.020 |
| closed types32×16 | 0.962 | 1.034 | 1.003 | 0.905 | 0.986 |

这些结果支持它作为减少机器输入工作的独立实现继续确认，但不是数量级改善或默认O2全形状胜出。随后的ca79 profile仍能看见同一LLVM热链；opaque8192的约44秒InstCombine也没有因scalar或再加early merge被解除。

## SLP消融纠正与LLVM实热点

第一次仅设置C API PipelineTuningOptions的SLP=false，被`default<O2>`解析覆盖。18构建/90 native语义控制有效，但“已关闭SLP”的消融解释无效，旧快照和失败测量不重标。后来绑定精确LLVM23.1.4源码，通过独立global gate在新进程选择开/关，以向量正例、标量反例和真实ZIL pass表证明执行门确实改变；对象缓存按加载器插入合同关闭，不能命中遗漏该研究flag的生产键。

实际opaque比较计划24构建，完成20 valid、100次五seed native和20 smoke；四个8192分支臂主动NotRun。控制器最后的FileExistsError/exit1是保留的停止机制，不是编译失败。2048默认分支的SLP CPU从2.3102降到0.0029秒，但JumpThreading仍约44秒；8192算术的InstCombine约44—46秒，而SLP仅很小部分。关闭SLP还把2048默认分支exe从383440增到403920字节，closed从146944增到167424字节。没有正式运行质量证据支持晋升该全局策略。

随后对自己的真实编译器进程做user-only perf，前三次在开始编译前因权限失败，第四次成功。冻结43037的438样本、lost=0中，162同时含`JumpThreading → MergeBasicBlockIntoOnlyPred → DTU::recalculate → CalculateFromScratch`，约占165个JT样本的98%。精确返回地址与上游源码把它关联到替换入口块后的完整支配树重算；0个LVI erase样本不证明零调用，也不足以支持其为此次主要热点。

ca79另一次冻结profile共549样本，157包含JT，152包含共同重算链，说明scalar后热点仍存在。两次profile分别绑定自己的ELF与策略，不合并为调用次数，也没有测出精确入口替换次数或二次复杂度指数。另一个ca79 opaque8192诊断的五输入均有效，InstCombine约43.93 CPU秒，提示还存在独立的算术优化热点。

由此形成的入口保留JumpThreading插件属于隔离研究：必须处理两侧LVI失效、可合并性、循环/迭代、WeakTrackingVH与Lazy DTU删除，不能写成“一行安全替换”。**插件完整CLI、默认/closed、资源与opaque运行质量、Rust比较的正式矩阵均待测；当前生产LLVM API与新的主分支整合身份没有借用上述profile或功能计时成为胜出结论。**

随后v1已完成单对象替换形成的两套真实LLVM库及ac4 API对照，保持上游class/header/layout；不是只依靠opt包装器。原版重编译与入口修复两臂都绑定61个组件输入、81个未改ScalarOpts成员、实际DSO/依赖和加载器身份。ac4两编译器仅额外加入私有pipeline/cache身份串，LLVMRunPasses实际pipeline不变；它们仍属于ac4，不因888测试修复而改标。

两个独审层次分开保留：plugin/wrapper的初版发布与错误协议曾判Fail，修复后重新核对逐键原子ZIP发布、原生失败、launch/timeout及损坏bundle；多键发布没有虚构为整体原子。真实native独审另执行45个C API请求、12个成功ZIL构建加1个当前类型拒绝，共105次native调用；13个真实源码请求都绑定实际编译器PID所加载DSO，缓存namespace、编辑/恢复/损坏恢复、当前Verify和producer边界分别核对。原版重编译在14个IR案例上与安装版相同，避免把“版本字符串相同”当完整基线。

这些是限定正确性进展，功能窗里的耗时不是正式端到端收益；完整EH传播、sanitizer/profile域及广泛运行质量并未验收。源码工具以09f1提交冻结，全部初始失败与修复、两独审、原input/oracle/ZIP/perf和精确来源见`llvm-entry`包。旧四次perf仍是官方baseline，不标成entry测量。后续SSA/LVI/CFG及组合候选另有身份，最终仍需冻结新组合编译器、实际加载库、pipeline与缓存身份再测完整矩阵。

## 继续拆解LLVM热点：LVI、CFG和SSA v1–v4

这些候选已经实际实现为隔离LLVM库，并分别完成限定独审。它们对应不同浪费，不能把各功能窗的阶段CPU比相乘成累计提速；后续组合独审另列，最终编译矩阵仍单独验证。下表全部是原先功能/诊断窗口，不是排他正式性能结论。

| 候选 | 实际改变与观测 | 负面与解释边界 |
|---|---|---|
| LVI值索引 | 在三个写入点维护value→hashed block-slot关系，eraseValue访问相关槽；同输入CVP CPU 18.8509→0.5466秒，两exe逐字相同 | JT仍39.96/41.67秒；索引含历史陈旧成员，内存不由当前live cache大小界定；单次进程峰值不能证明内存改善 |
| O2 IPSCCP后CFG清理 | 增加一次SimplifyCFG，提前移除常量传播后无用结构；算术InstCombine CPU 53.5422→0.3395秒，同输入exe逐字相同 | JT从0.0979增到1.2684秒，scalar入口热点基本未变；不是已证实全分布净益 |
| SSA v1 | 只在CFG/DT一致的threadEdge调用Bulk；保留其它调用点与debug/lifetime原路径 | 初次缺clang链接失败原样保留；有效retry中每变量live-region扫描仍在，JT约25秒属于旧功能窗 |
| SSA v2/v3 | 分别尝试共享未剪枝IDF和按需求闭包，并先检查PHI/use增长上限 | 两者在512-branch都拒绝全部505次helper；真实一处use需要两个PHI，阈值拒绝了必要SSA，无预期收益，不能改标成v4 |
| SSA v4 | 完整定义/使用块集合相同才复用原Bulk的精确pruned-IDF和reserve信息；不复用值/类型/通知 | 单2048-branch JT为5.2435 CPU秒，CVP仍19.3791秒；前述约25/42来自其它功能窗，不算匹配正式加速比 |

LVI独审执行48000次随机缓存变更/不变量检查、52个边界检查点、4次精确epoch拒绝，以及真实C API、当前ZIL Verify、loader和对象缓存控制。最初cache控制没有活跃predecessor事实的覆盖缺口由cache-v2补齐；最初两份profile准备因缺time工具和继承子进程最大RSS方法不适合而在启动编译器前拒绝，全部保留。最终诊断用精确wait4子进程资源，仍不能消除功能窗口并发影响。

CFG独审保留17个成功ZIL构建、2个当前类型拒绝、68次native及6个pipeline请求：O0/O3序列相同，O2只多目标pass。原profile因缺clang报E-LINK-IO，后面的v2才是有效重试，不能丢掉失败。原113798082字节stage stderr完整归档；380782521字节重复派生pass-tables.json只保存精确SHA，解析工具和原stderr可恢复。

SSA v4以相同flags重编译JumpThreading与SSAUpdaterBulk两单元，保持公共header/class/layout及原值重写、通知和PHI清理规则。独立v4审计为82个C API请求、134次native、39个helper及2个直接utility函数/34变量；v2的74个API/126次native属于其独立旧审计，不能相加冒充v4覆盖。v4另建计数库记录505个batch、18781变量、18276命中/505 miss、511838次live-in访问、37562 PHI，解释实际减少的结构工作；计数库时间不充当性能臂。初始helper链接、debug/lifetime夹具、错误CLI及v2/v3失败/无收益均保留。当前输入、完整32位oracle、clean-source重建和原ca79 perf也都可追溯。最终组合尚未借这些限定Pass获得正式速度或运行质量验收。

## 四单元组合的身份与正确性边界

74431冻结工具将入口保留JumpThreading、SSA v4、LVI索引和CFG位置改动合为四个LLVM翻译单元，原版与候选用相同flags构建；两侧真实ZIL API编译器均来自888源码，只改变四处私有pipeline/cache身份字符串。独审核对每侧661份构建输入与实际DSO加载；原版库SHA为`7046ae90…`，候选为`7b5c5ef8…`。它保留既有class/header/layout，没有以opt包装器替代真实LLVM API路径。

这次组合独审的范围是50个独立C API请求、31次成功ZIL构建、3次当前类型拒绝、186次原生调用和138752个生成的完整u32值；没有正式benchmark。作者另保留五个实际API profile和一次从源码重新构造库的控制，它们均是诊断/功能证据，不能作为正式编译比。原同源pair构建、原始IR/C控制和oracle、完整生成器、vendored C++、许可证、SDK安装/链接收据及原producer字节均进入`llvm-combined`包。fresh rebuild收据中的b733是被构建ZIL源码revision，不是组合生成器commit。

6ed研究workflow和manifest工具另有作者/独审协议控制。独审12种身份案例使用明确的非ELF假库/假archive，确认错误库、loader环境、失败命令、缺收据和错误SDK等被拒绝；这些假fixture原字节全部保存。它验证工作流与身份合同，不表示已实际加载LLVM或通过远端完整CI。原始工具审查、初始计时器失败和最终df04修复也独立归档；a638是较早Git revision，其bench文件SHA仍为c682，不能写成两个文件版本。

## 六族正式运行质量：原增量分区策略NoGo

冻结组合使用74431工具构造四单元LLVM库，实际API编译器共同源码为888；正式native窗口仍使用prepared中原runtime/merge-runtime/stage6_bench脚本，其中bench SHA为`c682809…`。后来的a638引用缓存审查和df04计时器修复不改写这个窗口身份。54次带loader审计的构建只是每artifact一次诊断准备；另有270次非计时原生正确性控制，之后排他执行1620次正式native（6族×9臂×5固定seed×6区组）。二者不能冒充正式编译速度或峰值内存对照。

非作者复核全部1620请求/54产物/48条真实ZIL loader记录、完整四字节stdout/空stderr、进程顺序与租约、原输入/工具/ELF/DSO身份；未额外重跑native。独立实现计算出270个arm中位数、300个重叠比较格及全部123个不利格，并匹配固定2000次按正反顺序分层的配对bootstrap。区间只反映这一个固定六区组窗口：五个seed不当独立区组，六个固定族也不当总体随机抽样，十组比较相互重叠且未作多重校正。

| native时间比较 | 六族GM比 | 探索性95%区间 | 不利比较格 |
|---|---:|---|---:|
| 默认candidate / 同源original | 0.480722 | [0.473208,0.489855] | 11/30 |
| closed candidate / 同源original closed | 0.485798 | [0.479883,0.495806] | 13/30 |
| 默认candidate / Rust | 0.699888 | [0.681539,0.715938] | 6/30 |
| closed candidate / Rust | 0.395792 | [0.384804,0.407182] | 1/30 |
| 原incremental / 默认candidate | 5.393289 | [5.305863,5.472825] | 22/30 |
| 原incremental closed / candidate closed | 9.581893 | [9.292923,9.711111] | 26/30 |

GM不能掩盖分布缺口。默认candidate的512-body仍比Rust慢28.786338倍，closed为0.942748；显式closed合同不能抵消默认合同的差距。原incremental在2048-branch相对默认candidate为26.792059倍，8192-arithmetic为326.301440倍；closed的8192-arithmetic仍为320.536744倍。即使全部输出一致，原分区/受限正文导入仍未满足同等级O2运行质量，已经判`NoGoForEqualQualityPromotion`。

因此这个不变策略原计划的23族增量编译1380请求及六opaque族增量编译360请求均在启动前标记NotRun，共1740；不把它们写作失败执行或完成样本，不以较快的编译换取未披露运行退化。后续质量修复使用新身份与新证据，不能覆盖本次NoGo。原分析的`cells.csv`、`all-adverse-cells.csv`、`summary.json`、独立`recomputed.json`和NoGo ledger都完整保留；小样本fresh native进程时间含启动/I/O/清理，不代表所有程序的稳态吞吐。

正式23族CLI的early/late编辑是既定函数的正文值修改，不是函数插入、删除或重命名性能覆盖。当前FunctionId及固定256函数owner划分仍可能随布局变化广泛失效。独立语义图、错误/恢复与模型控制能够证明其限定正确性，不能替代这些未测编辑类型的性能样本；计划中的8192-branch独立功能控制也不在执行前计Pass。

## 新组合默认合同的23族完整CLI窗口

这个窗口明确使用888共同源码的四单元候选LLVM、O2、after-MergeFunctions与可信原始语义候选缓存，关闭source-cache和incremental-o2；计时工具冻结在df04。Rust使用overflow checks、panic abort、8 CGU，并保留每个序列的增量缓存。六配对区组共1380请求，1320有效；`body-64x16`和`types-64x16`的60次ZIL预算拒绝原样保留，接受性未提高，只有21族/105单元有速度比。全部有效请求都有原生输出观察，630次成功ZIL均有当前独立Verify。

下表是完整CLI经过时间的ZIL/Rust几何比，越小越快；区间来自固定2000次正反顺序分层配对bootstrap，只描述该固定窗口。CLI启动、身份/缓存、前端、Verify、后端和链接都在计时内，源生成与原生程序运行在外。未改/restore是跨进程命中；不是函数级Types增量。

| 请求 | 21有效族GM | 探索性95%区间 | 较慢单元 | wait4峰值RSS比 |
|---|---:|---|---:|---:|
| cold | 1.062502 | [1.045326,1.082673] | 14/21 | 1.325173 |
| unchanged | 0.986217 | [0.964597,1.015480] | 11/21 | 0.960584 |
| early_edit | 1.520148 | [1.487865,1.555052] | 17/21 | 1.299844 |
| late_edit | 1.396218 | [1.363347,1.424426] | 15/21 | 1.281317 |
| restore | 0.728182 | [0.713961,0.745489] | 6/21 | 0.770216 |
| 全部有效单元 | 1.101225 | [1.088708,1.115823] | 63/105 | 1.103046 |

这些数据支持“有效固定语料上的完整O2请求已接近Rust总体量级”，仍不支持全面追平。大单模块8192函数cold为2997.8/2274.1ms，unchanged为639.1/413.6ms；前/后编辑仍为2.319/2.435倍，cold峰值RSS为1.602倍。body-16x16前编辑为2.514倍。restore的收益不能掩盖编辑与大模块差距，两个预算拒绝族也不能从总体适用范围中消失。closed和opaque窗口另行验证，不把不同窗口比值相乘或用单族区间作总体显著性声明。

复核按冻结runner再生编辑内容并与原SHA侧车核对，原root仅保留最终restore源码；再生内容不冒充每个历史请求的原始快照。runner没有记录逐请求ELF SHA；归档记录最终现存产物身份，不能追认历史ELF重哈希。DSO绑定来自pair原加载探针、编译器/launcher/library身份与对象报告，也不是逐请求LD_DEBUG审计。峰值RSS来自wait4，不与Session当前VmRSS或全系统同时峰值混比。

分析v1因SourceFileLoader在测量结束后生成一个pyc；该副作用按字节和时间保留。v2改为不写bytecode的加载，统计表、源清单和编译器审计与v1一致，原v1/pyc/raw未改。随后为下个窗口释放空间，先完整归档并核验，再只删除138个Rust增量缓存根的5172文件、4593946026逻辑字节；源码、program、对象、语义缓存和9160个非缓存文件保持一致。首次删除22项后因自身hardlink的ctime变化安全中止，修订后对剩余文件重新全SHA再完成；失败与修复均保留。这些清理不是性能样本，也未保存本来不存在的逐请求中间缓存状态。

## 闭合可执行合同的23族完整CLI窗口

相同冻结组合在显式closed-executable合同下另完成六配对区组，仍为1380请求、1320有效和两个族共60次预算拒绝；独审630次成功ZIL当前Verify与全部1320原生观察。分析使用已修复的无bytecode加载，没有新的pyc纠正epoch。比较的是本窗口内配对Rust，不能用默认窗和本窗的两个GM相除，伪造同一次实验中的选项提速。

| 请求 | 21有效族ZIL/Rust GM | 探索性95%区间 | 较慢单元 | wait4峰值RSS比 |
|---|---:|---|---:|---:|
| cold | 0.695917 | [0.682428,0.707841] | 0/21 | 1.289998 |
| unchanged | 0.987333 | [0.959033,1.018055] | 13/21 | 0.957318 |
| early_edit | 1.193007 | [1.169244,1.222330] | 14/21 | 1.258612 |
| late_edit | 1.117202 | [1.094293,1.145030] | 11/21 | 1.248511 |
| restore | 0.698480 | [0.683452,0.716517] | 3/21 | 0.771355 |
| 全部有效单元 | 0.914513 | [0.903484,0.926031] | 41/105 | 1.084019 |

cold在这21个有效固定族上的点估计均低于Rust，但cold峰值RSS几何比仍为1.290；未改和两种编辑没有全部追平。8192函数cold为2073.4/2458.6ms，未改为630.8/464.2ms，前/后编辑为1.396/1.482倍。body-16x16前编辑仍为2.392倍、未改为1.997倍。closed允许更强内部化，不把此合同下的结果宣称为默认导出合同的胜出；两个预算拒绝族、未校正多重比较及无历史逐请求ELF审计的边界与默认窗一致。

原数据和分析冻结后，另对实际138个Rust缓存根、5172文件、4593942570逻辑字节逐SHA验证并回收；9159个非缓存文件前后完整stat集合相同。该次直接采用硬链接修复后的单epoch，首次执行即完成，未把default旧失败混入closed结果。空间回收只服务后续测量，不改变本窗统计或补造中间缓存状态。

## 14个仓库真实小项目的完整CLI窗口

默认导出合同下的真实行为配对另完成336请求：14项目×6区组×ZIL/Rust×cold/unchanged，全部有效。168次成功ZIL均做当前Verify；5040次原生观察与210份单独oracle结果匹配。这里的Rust是独立签入的行为对应实现，不能推定两种语言的内部编译工作相同；app的Rust使用`--check`，有效原生结果也不一律要求退出码0。

| 请求 | 14项目ZIL/Rust GM | 探索性95%区间 | 较慢单元 | wait4峰值RSS比 |
|---|---:|---|---:|---:|
| cold | 0.473680 | [0.465546,0.493740] | 0/14 | 0.762310 |
| unchanged | 0.542046 | [0.519243,0.554372] | 0/14 | 0.564834 |
| 全部单元 | 0.506711 | [0.496636,0.520065] | 0/28 | 0.656185 |

这是14个固定仓库小项目上的完整请求收益，不推广为工业大项目普遍结论，也没有编辑请求结果。冷热均指编译器缓存，cold不等于OS页缓存清空。仍使用六区组固定2000次配对bootstrap；各单元区间没有多重比较校正。原工具分别冻结为real runner `dfd9ea08…`、distribution helper `73c0be9e…`、实际计时helper `8be9d6cf…`，不借后续工具身份。独审核对最终unchanged ELF，cold ELF只保留历史行内SHA，未声称重哈希已覆盖产物；原生完整输出在row/oracle JSON中，DSO仍由pair探针和对象报告绑定。

## df04官方与研究LLVM的双CI验收

精确`df04d4920b9d3ec618e660f8149063eb6019b317`的[研究LLVM运行41212317882](https://github.com/zil-lang/zil/actions/runs/41212317882)与[官方运行41212324365](https://github.com/zil-lang/zil/actions/runs/41212324365)均为completed/success。完整原日志显示研究侧SDK/header、四单元库重建及身份收据Pass，随后执行workspace、reference、Clippy和LLVM text-reference等工作流门禁；官方同HEAD门禁也成功。这次是实际远端工作流结果，和此前仅假fixture的身份协议审查各有独立范围。

两份完整日志ZIP的CRC和每成员SHA均核对，原run/jobs/artifact-list元数据一起保留；artifact列表不冒充已下载其单独payload。归档包含df04的两workflow、精确producer、vendored C++/许可证和SDK重建输入74份源码。两个Pass不属于后续quality-closure候选或当前HEAD，旧2e8成功与更早失败证据保持原身份，不能覆盖重写。

## O2质量闭合v1：修复思路与限定正确性

原incremental运行质量NoGo之后，新的隔离v1候选针对两个具体缺口继续实施：受限正文导入可能截断跨单元直接调用链，available_externally正文也不是每个消费者内可靠保留的内部实现。候选为消费者规划完整的可支持直接调用闭包，生成新Internal实现名，只改直接callee；原canonical函数实体、地址、参数/返回值、间接调用与全局状态仍保留。递归由visited终止；遇跨owner的未知编译器实现、私有存储依赖、名字冲突或整体复制费用超限，回退原完整模块，不默默截断闭包。

复制费用按每个消费者新增的指令/块成本计数，总新增成本不超过原source函数成本，等于边界时接受；这不是两倍文本、RSS或LLVM优化时间的保证。最后消费者取得原值、其他消费者取得未修改模板的副本，避免先处理的canonical owner或clone污染后续消费者。缓存仍以完整变换后单元及声明依赖为输入；这改善跨单元机器优化可见性，不是Types/CT语义增量。

真实API输入捕获的原receipt中`capture_count:false`保持不变：初版错预期每请求16次调用，实际8个CGU各有完整before/after/meta，API以一次default<O2>执行每单元并设置MergeFunctions选项。修正说明也保留此前误称“另有legacy Merge调用”的解释错误，没有把解释修订当重新编译。原始捕获、bounded/full与AE/internal的真实IR消融、默认/closed body对照、snapshot构建和作者30次构建/150次native都属于功能/诊断窗口，不提供正式速度或广泛运行质量结论。

最终v1独审绑定888基础、661份Git输入、实际5ca5…机器划分源码、1f2f…编译器和既有四单元7b5c…DSO；13个独立结构控制覆盖三个owner位置的1535/1536/1537费用、未知helper/私有状态/名字冲突回退及closed地址。另有14次真实ZIL构建、1次当前拒绝、70次原生执行；跨单元受影响编辑为3miss/1hit，无关第四单元编辑为1miss/3hit。递归、函数值、资源/Drop回退、实际loader和当前Verify均有独立控制。

作者测试先因借用/移动错误失败，测试专用`name.clone()`修正后18项通过；原BUILD/15759来源及初始测试字节和后续TEST-SUPPLEMENT分开绑定，不能把测试修正后的快照重标为原编译ELF来源。独审初始panic策略、cache-hit字段及源码格式假设失败也保留。更早的4cab…结构静态审查版本只保存报告/SHA，没有另存完整源码字节，不能声称完整恢复该早期状态。最终v1可恢复输入与旧producer快照已封存于`quality-closure-v1`；主树生产a1ab另有身份与审查，本段不批准它，也不覆盖原326倍NoGo或预填新正式矩阵。

## 同源LLVM隔离：旧888完整CLI收益的幅度

最后一个旧888隔离窗以相同ZIL生产源码比较原版与四单元候选LLVM，工具仍df04，没有Rust臂。23族×6区组×5请求×两ZIL共1380请求；1260有效，body/types两拒绝族在两臂合计120次预算拒绝全部保留。1260次成功都有当前Verify与native观察，对象报告为5736hit/2064miss。独立分析按原始行和同一配对方法复算，不能把这个比值改叫ZIL/Rust。

| 请求 | 21有效族candidate/original GM | 探索性95%区间 | 较慢单元 |
|---|---:|---|---:|
| cold | 0.958337 | [0.944967,0.973135] | 9/21 |
| unchanged | 0.993850 | [0.979411,1.007000] | 9/21 |
| early_edit | 0.943154 | [0.931690,0.961152] | 8/21 |
| late_edit | 0.958824 | [0.942958,0.972567] | 6/21 |
| restore | 1.003497 | [0.984172,1.016109] | 12/21 |
| 全部有效单元 | 0.971259 | [0.964327,0.977731] | 44/105 |

完整窗口总体收益有限，峰值RSS整体几何比1.005370；大单模块有更明显改善：8192函数cold为3027.1/4526.3ms，前/后编辑为3052.7/4542.6及3031.9/4528.7ms，比值0.6688/0.6720/0.6695。但restore总体1.0035，仍有44个不利点估计；未经多重校正的7不利/20有利区间不作总体显著性判定。这份正式归因不能与opaque单pass诊断倍数混用，更不能改标为随后a1ab质量闭合生产版本。

## a1ab质量闭合生产整合的限定验收

生产`a1ab243fa5ba1b9841c904d251fea7c999aa6681`已另行构建，实际编译器为cdc6…、所加载LLVM仍7b5…；662份来源与构建前后Git一致性、命令、loader及wrapper绑定独立保留。原实验API私有标签不借作生产语义；对象action协议为`ZIL-OBJECT-ACTION-212-v2-complete-direct-call-closures`，旧实验缓存按新身份隔离。

作者18项限定backend测试通过。最终独审另执行8次最终ELF成功构建、2次旧实验缓存对照、40+10次native和3次产物保护负例；旧13个结构控制按精确等价复用，新增结构执行为0。它证明所列源码/缓存/错误控制，不提供正式编译速度或运行质量恢复结论，也不继承df04远端CI为a1ab完整CI。

`ENV-SUPPLEMENT`在构建完成后记录同环境脚本、显式覆盖、工具派发及部分环境/配置存在性；原cargo启动时没有捕获这些值，完整环境及配置内容也未保存。补收据不追认构建时观察，不改写BUILD。新的900请求质量/编译矩阵及8192branch控制须按后续实际身份和状态填写，当前不预填Pass。原326倍NoGo、v1诊断与这里的生产正确性继续分别保留。

## 原始表与失败记录索引

所有包都保留原始身份。schema-1的thin/screens/correctness/merge-screens用`stage6_archive.py`，schema-2其余新包用`stage6_archive_large.py`：`--extract <包目录> --dest <新目录>`恢复下表的逻辑路径，`--verify <包目录>`核对。algorithms包是其原有多tar档案。README、IDENTITY、VERIFY与完整FILES/OMITTED说明保留范围；ELF/缓存二进制省略项仍有原字节SHA。压缩和内容去重没有改写统计行或失败状态。

下列链接均相对本页指向仓库内归档；“内部表”是恢复后的精确路径/命名规则，不是只给出选择性截图。

| 包 | 完整原始表/收据位置 |
|---|---|
| [algorithms](../../../evidence/research/incremental-compilation/stage6/algorithms) | 顶层`stage6-graph-final-v2-summary.json`、`stage6-factor-benchmark-summary.json`、`zit-benchmark.json`；各tar中的逐臂JSON/JSONL、prepare、失败与关系oracle |
| [screens](../../../evidence/research/incremental-compilation/stage6/screens) | `research/stage6/screen-{default,closed,isolation}/{summary.json,rows.jsonl,manifest.json}`；540请求/135单元 |
| [thin](../../../evidence/research/incremental-compilation/stage6/thin) | `research/stage6/thin-screen-{default,closed}-2block/`及`thin-screen-body-{default,closed}-2block/`的summary/rows；`thin-author.md`、各初始失败epoch和修复控制 |
| [merge-screens](../../../evidence/research/incremental-compilation/stage6/merge-screens) | 三完整窗summary/rows及第四窗rows/STOP.json；`merge-screen-execution.json`、包外COMPLETION.json保留535/1/104 |
| [scalar-screens](../../../evidence/research/incremental-compilation/stage6/scalar-screens) | `research/stage6/scalar-screen-{default,closed}/{summary.json,rows.jsonl}`与`scalar-screen-execution.json`；全部80时间/RSS单元 |
| [session](../../../evidence/research/incremental-compilation/stage6/session) | 两正式根`session-db-isolation`、`session-scalar-vs-3d6`的summary/rows/execution/manifest及所有源码/控制；旧529根仅SupersededPrepared |
| [session-review](../../../evidence/research/incremental-compilation/stage6/session-review) | `research/stage6-session-review/{SESSION-REVIEW-TABLES.md,recomputed.json,recompute.py,review.json,inventory.json}`；全部140单元与探索性区间，初次复算失败log保留 |
| [correctness](../../../evidence/research/incremental-compilation/stage6/correctness) | 原Analyze/DB、semantic、Merge及algorithms独审；`stage6-merge-tools/shape-functional-audit.json`；作者失败/修复、Session准备及旧DB来源 |
| [scalar-correctness](../../../evidence/research/incremental-compilation/stage6/scalar-correctness) | `research/stage6-scalar-review/`和scalar作者/release身份；`.exe.ll`、差分源、native结果与Clippy |
| [diagnostics](../../../evidence/research/incremental-compilation/stage6/diagnostics) | pass-timing summary/原表；`slp-effective-opaque/{final-summary.json,ADAPTIVE_STOP.json}`；四次perf尝试、真实perf.data/调用栈；ca79两个profile、精确LLVM源码及原生stdin |
| [syntax-correctness](../../../evidence/research/incremental-compilation/stage6/syntax-correctness) | `stage6-project-syntax/`作者失败/修复和root身份；`stage6-syntax-review/`独审；baseline SOURCE/release。root Clippy/fmt字段明确是attestation而非原始log |
| [syntax-session](../../../evidence/research/incremental-compilation/stage6/syntax-session) | `research/stage6/session-syntax-{isolation,vs-3d6}/`逐请求rows/stdout/process/源码/控制，以及准备、独占窗口和构建身份；两窗共1680请求 |
| [syntax-session-review](../../../evidence/research/incremental-compilation/stage6/syntax-session-review) | `research/stage6-syntax-session-review/{recomputed.json,review.md,request-audit.jsonl,control-audit.jsonl,inventory.json,recompute.py}`；全部140时间/RSS单元及配对区间，首次review schema错误脚本/log保留 |
| [integration-correctness](../../../evidence/research/incremental-compilation/stage6/integration-correctness) | `research/stage6/integration-checks/`的75项、Clippy/fmt、ac4/888身份与真实CI失败；`stage6-integration-review/`的3项/29报告对、夹具失败和DB补审；upstream四静态文件；旧Rust缓存清理中断/完成收据 |
| [ci-disk](../../../evidence/research/incremental-compilation/stage6/ci-disk) | `research/stage6/ci-disk-recovery*/`全部作者/独审、sudo父目录及umask修订；412072原始失败/空日志；412093真实清理JSON/ZIP，整次CI未计Pass |
| [llvm-entry](../../../evidence/research/incremental-compilation/stage6/llvm-entry) | v1 generated/native源码与patch、原版/entry DSO及ac4 API pair身份；完整plugin失败/修复/opaque/resource控制；`stage6-jt-review`与`stage6-native-jt-review`；42原ZIP、2input/2oracle与旧baseline四perf |
| [llvm-lvi](../../../evidence/research/incremental-compilation/stage6/llvm-lvi) | `lvi-value-index*`全部原始/失败/profile/cache-v2与`stage6-lvi-review`；生成源码/license/patch、原输入和完整oracle |
| [llvm-cfg](../../../evidence/research/incremental-compilation/stage6/llvm-cfg) | `post-ipsccp-cfg*`、arithmetic IR/stage/CFG原始诊断和两独审；原113.8MB stderr保留，380.8MB派生表仅SHA |
| [llvm-ssa](../../../evidence/research/incremental-compilation/stage6/llvm-ssa) | 完整`ssa-update` v1–v4源/失败/计数/有效重试/clean重建；独审v2/v4、14份原input/oracle及原ca79 perf |
| [screen-cache-cleanup](../../../evidence/research/incremental-compilation/stage6/screen-cache-cleanup) | 36个旧screen Rust缓存的计划、1112文件完整SHA匹配/删除audit；初次schema读取失败在删除前退出，正确重试回收1152760644字节 |
| [ci-main-412093](../../../evidence/research/incremental-compilation/stage6/ci-main-412093) | 精确2e8 HEAD的官方成功CI，原run/jobs/artifact-list和503718字节完整日志ZIP；不接受后来的组合或当前HEAD |
| [llvm-combined](../../../evidence/research/incremental-compilation/stage6/llvm-combined) | `research/stage6/combined-v1-*`源/同源pair/构建/控制/五profile，fresh rebuild与旧producer；`stage6-combined-review`全部独审；744完整工具/C++/许可证、SDK收据及借用原始控制输入 |
| [research-ci-tools](../../../evidence/research/incremental-compilation/stage6/research-ci-tools) | 6ed workflow/manifest/提案精确字节，`research-llvm-ci`作者与`research-llvm-ci-independent-v1`12案例；假库与负例原字节，不是远端CI通过 |
| [benchmark-tools](../../../evidence/research/incremental-compilation/stage6/benchmark-tools) | confirmation/timer/opaque/runtime工具作者及独审所有初始失败和修复；旧c17、7830、a638与df04源及其import依赖；首次未冻capture缺helper的纠正收据 |
| [runtime-final-v1](../../../evidence/research/incremental-compilation/stage6/runtime-final-v1) | `combined-runtime-prepared-v1`全部54产物身份与1620逐请求原输出；270额外oracle、旧/新producer控制、lease、分析全表与2f8独立复算；原incremental1740NotRun ledger |
| [confirmation-default](../../../evidence/research/incremental-compilation/stage6/confirmation-default) | 默认23族1380请求的完整rows/源身份/原runner/命令/lease及分析v1；1320有效+60预算拒绝，全部115单元状态及105有效比值；所有最终Rust缓存逐文件SHA |
| [confirmation-default-cache-cleanup](../../../evidence/research/incremental-compilation/stage6/confirmation-default-cache-cleanup) | v2无bytecode分析与副作用纠正；138缓存根5172文件的SHA核对/删除audit，初次22项中断、hardlink诊断、重验5150项与完成收据 |
| [confirmation-closed](../../../evidence/research/incremental-compilation/stage6/confirmation-closed) | 显式closed合同23族1380请求、源/工具/pair身份、完整stdout/stderr/命令/lease与独立分析v1；60预算拒绝和115单元状态均保留 |
| [confirmation-closed-cache-cleanup](../../../evidence/research/incremental-compilation/stage6/confirmation-closed-cache-cleanup) | 138根5172文件的原始清点、全SHA核验、硬链接重验与精确删除；9159个非缓存文件保持一致 |
| [confirmation-real](../../../evidence/research/incremental-compilation/stage6/confirmation-real) | 14真实小项目336请求、5040原生观察与210oracle、原helper/source/命令/lease及独立分析v1；28完整比较单元，cold历史SHA与最终ELF边界明确 |
| [ci-dual-df04](../../../evidence/research/incremental-compilation/stage6/ci-dual-df04) | df04研究/官方两成功run的原API元数据与完整ZIP日志、每成员SHA/CRC核对；74份精确workflow/producer/C++/许可证/SDK重建源码 |
| [quality-closure-v1](../../../evidence/research/incremental-compilation/stage6/quality-closure-v1) | 原Fail捕获/完整actualIR/ablation/body对照/30build150native；旧producer与9工具快照、15759来源/测试修补、独审13model/14build/1reject/70native；早期4cab仅报告SHA边界 |
| [confirmation-isolation](../../../evidence/research/incremental-compilation/stage6/confirmation-isolation) | 旧888同源原版/候选LLVM1380请求，1260有效/120预算拒绝，全部115状态/105比值、原source/runner/lease/命令及独立分析v1；没有Rust臂 |
| [quality-final-a1ab](../../../evidence/research/incremental-compilation/stage6/quality-final-a1ab) | 最终a1ab生产SOURCE/BUILD/18tests/loader及旧producer，构建后ENV补收据；独审8最终+2旧cache/50native/3负例，独立于v1且不含性能验收 |







## 待完成的确认，不预填结果

| 新窗口/工作 | 本稿状态 | 完成后必须回答的问题 |
|---|---|---|
| 最终整合身份的Session | ffe4两窗已完成；不得自动改标为新身份 | 新main/LLVM变动是否影响既有编辑收益、常驻内存及错误/取消/预算控制 |
| 合入f257后的生产CLI与Rust同工作负载矩阵 | ac4/888限定源码与CLI已冻结；默认与closed各23族、14真实项目的六区组已完成并独立审核；旧888同源隔离也已完成；新a1ab/opaque质量矩阵仍待冻结结论 | 默认与closed、冷/暖/前后编辑/恢复，全部形状和失败；同等级优化下是否缩小O2差距 |
| 运行质量后续确认 | 六族旧组合已正式完成，原incremental质量NoGo；真实项目及新修复仍待测 | 保留默认body差距和全部不利格；新策略必须用独立身份证明运行质量，不能借旧GM晋升 |
| 入口保留JumpThreading库候选 | 真实API四单元组合独审及六族runtime已完成；最终编译矩阵待发布 | 原版clone/内建等价、class/layout及实际加载库身份、LLVM验证、入口/循环/资源控制，以及完整请求和运行质量能否复现收益 |
| 稳定语义owner与更细Types/CT/函数依赖 | 尚未完成 | 真正减少实际语义工作，并保留当前源码、ID、查找、规则和预算有效性，而非只改缓存计数 |

四目标仍分别判断：已完成的安全证据是限定源码/错误/恢复和独立Verify控制，不是整语言最终安全验收；运行性能仍需上述质量矩阵；本轮没有以语法便利替代性能证明；编译方面已取得若干分布上的实际改善、容量扩展和具体热点定位，也完整保留了回退与撤回。最终突破结论必须落到新身份、完整用户请求及可接受运行质量上。
