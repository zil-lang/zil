# 第六阶段结果：增量边界、O2 差距与候选取舍

本轮已经把旧图算法、修复适配后的 zit、生产 DB、跨进程语义候选、标量发射、ThinLTO、MergeFunctions 和 SLP 分别实施或实际消融。结果支持继续减少真实前端工作、改善机器输入，并攻克已定位的 LLVM 热点；同时淘汰了没有净收益的反向 DB 和灾难性退化的 early/closed 合并组合。当前证据尚不能宣布通用编译速度突破、默认 O2 全面追平 Rust，或运行质量已经全面相当。

本页是**已冻结结果稿**，不是最终组合交付验收。专项正在吸收 main 的 `f257…` 更新，方向是合入专项分支，不是把专项合入 main；最终生产身份和相应完整矩阵将另行冻结。下面每个数字仍属于原来的源码、工具、ELF、输入和窗口，不能重标为合并后的新二进制，也不能相乘成为“累计提速”。README 入口暂不切换。测量定义见 [STAGE6-PROTOCOL.md](STAGE6-PROTOCOL.md)。

## 已经作出的取舍

| 路线 | 本轮实际进展 | 已冻结结果与当前边界 |
|---|---|---|
| 旧076局部域/图方法 | 适配当前前端，重跑26族；直接、intern、BDD fused/cut均有真实尝试 | 16资源形状的局部直接查询核约1.90倍速度；分支存在回退，大根函数仍受表示/枚举预算限制。收益不能归给BDD切割或完整编译 |
| 事件支持因子化 | 从真实TypedIR转移直接求解，保留完整sum-of-products联合关系 | 能保留128个独立bool的入口乘积；完整资源查询最坏回退6.540倍。容量扩展成立，通用速度优势不成立 |
| zit适配与生产反向DB | 修复精确环检测、拓扑秩、失败恢复；生产接入反向边和相等屏障后做隔离消融 | JS适配开销明显增加；生产DB六区组未证明净收益，默认DB已完整恢复3d6原实现 |
| 真正减少前端工作 | 保留初始化纯投影；新增可信本地原始TypedProgram缓存；ffe4保留项目原始Parse Arc | 前两者分别是有限子分析复用和整项目精确候选复用；ffe4实际复用未改模块的parser结果。均不等于函数级Types增量 |
| 显式O2分区/正文导入 | 固定owner块、唯一canonical定义、受限优化副本及真实对象键 | 旧EE两区组中有明显编辑收益，也有内存与暖请求不利点；运行质量与最终Rust矩阵仍须确认 |
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

下一步计划把入口修复放入单对象替换形成的真实LLVM库，沿原LLVM API路径加载，并保持上游class/layout。隔离opt插件只提供研究对照，不能独自证明这条实际路径已经完成。最终需要同时冻结新组合编译器、实际加载库、pipeline与缓存身份，再做完整矩阵；这些工作尚不能写成成功结果。

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

## 待完成的确认，不预填结果

| 新窗口/工作 | 本稿状态 | 完成后必须回答的问题 |
|---|---|---|
| 最终整合身份的Session | ffe4两窗已完成；不得自动改标为新身份 | 新main/LLVM变动是否影响既有编辑收益、常驻内存及错误/取消/预算控制 |
| 合入f257后的生产CLI与Rust同工作负载矩阵 | 最终组合二进制及实际LLVM工具链身份尚需新建，结果待测 | 默认与closed、冷/暖/前后编辑/恢复，全部形状和失败；同等级优化下是否缩小O2差距 |
| opaque与真实项目运行质量矩阵 | 待正式结果 | 正确输出之外的吞吐、代码尺寸和资源行为；导入上限/SLP/Merge等策略是否损失运行性能 |
| 入口保留JumpThreading库候选 | 隔离plugin已有研究用途；真实API库路径及正式矩阵待测 | 原版clone/内建等价、class/layout及实际加载库身份、LLVM验证、入口/循环/资源控制，以及完整请求和运行质量能否复现收益 |
| 稳定语义owner与更细Types/CT/函数依赖 | 尚未完成 | 真正减少实际语义工作，并保留当前源码、ID、查找、规则和预算有效性，而非只改缓存计数 |

四目标仍分别判断：已完成的安全证据是限定源码/错误/恢复和独立Verify控制，不是整语言最终安全验收；运行性能仍需上述质量矩阵；本轮没有以语法便利替代性能证明；编译方面已取得若干分布上的实际改善、容量扩展和具体热点定位，也完整保留了回退与撤回。最终突破结论必须落到新身份、完整用户请求及可接受运行质量上。
