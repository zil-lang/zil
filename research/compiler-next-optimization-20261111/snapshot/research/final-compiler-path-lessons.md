# 编译路径本轮复核：已吸收的机制、已证边界与收束

## 当前结论与待定项

本轮全部候选均未采用，main不合入这些研究变更。**native2321的运行质量Pass和完整编译NoGo同时成立；最后full2458虽有N256局部编译改善，却因两个case运行质量均NoGo而不能采用。** 本文不提出新实现或新测量。

最后对照固定为：producer `2458defcaf8be51c87b80c0dadf391bb585735e8` / tree `e917926bc8976da90e58897911c72978b9a24f88`，protocol `360635bffc5dc4230bd2c9ed2ca18e66c8edd29139e17a910afc559c03c0147f`，raw `out/next-optimization/full-producer-screen-v1`。**状态：36次完整编译／108次native已完成，exit1为运行质量NoGo，不是Incomplete。** 以下full数值及完整性现已由Blaze代理完成独立raw审计，结论为Complete、质量NoGo；本报告仅引用该独审，不重复大hash，也不冒称自身逐raw复算。详[full最终独审](full-producer-result-review.md)，冻结JSON SHA256 `e6280cf7b6a00144e6643639d70c93656f6830b868ff3ea2ec4ccd2f56b0a23f`。源码可行性结论见[精确delta独审](manual-prelink-full-source-review.md)，不替代运行证据。

## 一、Rust与Blaze实际吸收了什么

Rust带来的依据是三个层次：实例身份与CGU owner分开；前端local copy与LLVM实际跨模块导入分开；post-LTO复用仍依赖当前模块、imports及工具/目标/policy身份。ZIL据此尝试canonical owner先优化，再按真实调用需求消费body，保留原唯一身份、原budget和当前完整验证。没有搬入Rust全部stable/volatile分桶，没有实现rustc的ThinLocal索引/缓存，也没有借Rust允许函数指针非唯一的契约放松ZIL身份。详[本轮Rust源码核查](rust-next.md)与固定源码：[MonoItem与InstantiationMode](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_middle/src/mono.rs)、[partitioning](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_monomorphize/src/partitioning.rs)、[ThinLTO复用](https://github.com/rust-lang/rust/blob/9de746938314271db6dbb62dc3b4f07288d07d54/compiler/rustc_codegen_llvm/src/back/lto.rs)。这些是源码机制引用，不是本轮执行rustc该main commit的性能证明。

Blaze带来的具体机制是：一个producer形成已验证、已优化的owned bitcode及facts，importer消费同一产物；工具/target/全部优化依赖进入精确身份；多view与同key在途复用有明确调度和发布合同。ZIL2321实际落地的是**请求内canonical prelink复用**，全producer阶段join后只把immutable owned bytes/facts送到目的worker；没有照搬daemon、函数级memo、shape-template、BestOf或single-flight，也没有持久prelink cache。详[Blaze源码核查](blaze-next.md)；实际release主链接入在[v2/release.rs](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/v2/release.rs)，负责summarize/Imports/emit及Kept/Export共享；它调用底层[llvm/release.rs](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/hir-ty-codegen/llvm/release.rs)的prelink/compile、own/export Arc与实际postlink。两层职责分开，另见[flight.rs](https://github.com/indexable-inc/blaze/blob/ae1a7a7a1771c1658173ee4829392c40b4f1d4cc/crates/blaze-codegen/src/flight.rs)。

必须纠正两种类比：Blaze正常release imports实际是AvailableExternally，不是本轮ZIL的Internal；它按函数prelink，ZIL本轮仍按8个canonical CGU，两者的早期内联与body形状不同。Blaze源码/模型backend测试也不能证明ZIL真实LLVM并发wall。其O0/O1 stencil、live/tiered响应不作为本轮完整O2 AOT质量的替代依据。[实际生产链边界](blaze-next.md#4-当前优化边界与运行质量文档实现必须分开)

真正保留的共同原则是“复用精确相同的已完成后端工作”，不是“把检查关掉后更快”。现有ZIL已经具备一次编最终快照、当前parse借用及完整object cache；不能将它们重新命名为本轮新增批量优化。[当前主线后端](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/build.rs)、[cache身份及发布](https://github.com/zil-lang/zil/blob/99035931d358add0eaed6d4535035c38413701dd/crates/zil-backend-llvm/src/object_cache.rs)

## 二、为什么2321质量Pass仍编译NoGo

2321 / tree03fa使用真实同进程C API，保留双整图与当前unit预检、原Complete planner typed proof、actual Required⊆Allowed、精确donor裁剪/链接、逐Use与类型/metadata检查，以及完整O2。静审与实际64 API tests、6 proof、no-native check和release通过，说明新增边界可运行；它们不证明完整请求更快。[源码独审](manual-prelink-2321-source-review.md)

a299正式screen完整36次编译、108次native，所有功能输出正确，两个case运行门通过；编译结果却是：

| case | 编译A/B mean wall（ms） | B/A wall | B/A CPU | 运行B/A | 运行Rust/B |
| --- | ---: | ---: | ---: | ---: | ---: |
| N64/base | 1166.246 / 1428.280 | 1.2246814 | 1.0176928 | 0.9920555 | 4.1829151 |
| N256/base | 4456.924 / 4867.913 | 1.0922136 | 1.0540271 | 1.0519041 | 4.5462097 |

因此N256运行仍慢5.19%，虽在1.10门内也不能隐去；两格编译wall与CPU均回退，不能被“仍比Rust生成程序跑得快”抵消。该Rust/B是**生成程序运行wall比**，不是编译速度比。详[非作者逐raw复核](manual-prelink-result-review.md)，protocol `a2999b0eb482994de4516f8de6760ca6dd4a7b99fe722c8b0d75aaa9daa76810`。

机械上移除了Python wrapper、外部opt/dis/dump、反复大工具hash、lld thinlink/index/plugin并不自动消除全部新增工作：2321八个unit仍各做一次thin-prelink O2，然后全部再做thinlto O2；另外有BC序列化、own parse、root按7个donor parse/DCE/link、多次scan/verify与阶段屏障。其真实图根Required为56/224，另七unit Required空。**这些是实际存在的工作，不是各项耗时的独立因果分摊**；当前不能把NoGo全归于第二O2，也不能从stage wall总和减出潜在收益。[C API路径与成本边界](manual-prelink-c-api-design.md)、[实际图复算](manual-prelink-result-review.md#原生两阶段与独立依赖图)

full2458以新冻结源判别一个具体冗余：producer真实完整default<O2>，实际Required空时保留当前图/parse/target/sealed/LLVM验证后直接发射，非空仍完整导入和thinlto O2。[full源码独审](manual-prelink-full-source-review.md)

Root完成通报并经Blaze独立全raw复核的最终结果如下：

| case | 编译A/B/R mean wall（ms） | 编译B/A wall | 编译B/A CPU | 运行B/A |
| --- | ---: | ---: | ---: | ---: |
| N64/base | 1165.224937 / 1447.139819 / 902.797089 | 1.24194031 | 1.00798963 | 1.344953924 |
| N256/base | 4486.5228995 / 4061.06725 / 2828.954319 | 0.905170294 | 0.867540583 | 1.462278819 |

Blaze独审确认913份raw前后SHA相同、146份attempt正常；96份full producer及192项before/after验证齐全，12个根目的实际运行postlink、84个空Required目的未运行，1680个imports/direct Uses。每case六个非空及42个空目的unit；本screen没有“Allowed非空但Required空”的实际unit，这一最近反例只由API控制覆盖，不能把测试覆盖写成本screen观测。[full最终独审](full-producer-result-review.md)

N64编译仍慢，N256局部编译wall约−9.48%、CPU约−13.25%；但运行分别慢约34.50%和46.23%，均超过原1.10门，因此整体NoGo。不能挑N256编译格作为可采用收益，也不能因exit1抹去完整36／108结果。full-O2 producer与空Required复用共同改变了管线和最终代码质量，现有对照没有隔离具体pass因果；**不能断言仅skip第二O2导致退化，也不能断言某个inliner／向量化pass独自负责。** 旧2321质量Pass不适用于新管线。

## 三、这次定位得到什么，不能外推什么

默认ThinLTO出现零实际导入；严格完整可见性使AE body真正进入`.3`，但仍未inline。一次固定输入的官方inline remarks诊断显示，AE导入body被TooCostly拒绝，同体量local sole-call得到15000奖励。这给private-import对照提供了实际机制依据；不是凭名字猜AE完全不能优化——strict阶段return range已成功消掉部分overflow，内联与跨leaf组合优化仍未恢复。官方[InlineCost.cpp](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Analysis/InlineCost.cpp)、[TTI默认bonus](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/Analysis/TargetTransformInfoImpl.h)；实际证据见[inline诊断](thinlto-complete-visibility/inline-diagnostic-report.md)与[strict/default IR对照](thinlto-complete-visibility/strict-default-ir-comparison.md)。

身份去重与Counter确实删除了研究执行器中的重复工作：首核固定10次完整hash、10 invocation paths/9 resolved物理文件，后续guard；N256每轮224×51547次逐行regex改为51547次。但identity、linear各自完整screen仍编译NoGo，且它们是不同session，不能把跨session的均值差额当成配对提速。不能从剩余父进程CPU包络断言全部是hash或regex。[成本只读分解](private-compile-cost-audit.md)、[Counter等价与操作量](private-linear-scan-audit.md)、[linear实际结果](thinlto-private-linear/result-review.md)

同样，结构sink与builder插入点缓存都保持同ELF/materialIR，却没有完整编译净收益：固定84请求工作总量wall分别+7.632591%与+2.078146%。这是各自screen总工作量，不是原发生频率加权score，也不支持只挑某个暖格采用。[sink结果](structured-emission/result-review.md)、[builder结果](builder-position/result-review.md)

## 四、两个pure冷case不足证明LLM多点／并发优势

本轮N64/256都是固定base、8CGU/2CPU、每arm六个独立完整cold请求，实际没有批量编辑、warm/resident复用、跨请求cache、并发候选或共享memo。源码文件多不等于测试过“一次改16/64文件×多位置”；一份base编译重复六遍也不等于六个不同agent候选。编译36条与native108条不能混作36次多候选批次。

native质量证据只覆盖这两种纯算术/直接调用程序及三输入模型。没有闭包、可变global、函数地址、foreign callback、复杂ABI的产品质量结论，也未重证旧八case和原完整频率门。原始小n6的p95是该样本描述，不是稳定尾延迟；RSS为wait4高水位且有fork floor，不是并发同时峰值。Rust1.99.1实际bundled LLVM23.1.1，ZIL/Clang为23.1.4，不能称二者完全相同LLVM patch实现。[a299完整方法与限制](manual-prelink-result-review.md)

## 五、已有main暖请求与c4究竟覆盖什么

主线暖cache已有真实证据：缓存命中仍走当前Verify和完整machine preflight，旧producer LLVM验证不冒充本请求LLVM builder重跑；暖请求仍承担lower、fold规划、layout key/replay、unit准备等成本。84请求中的shared4096/shared1024/owned8及private/edit-warm覆盖既有选定工作形状，不能代表全部LLM修改分布，也不是本次native candidate的warm结果。[暖路径审计](warm-request-redundancy-audit.md)、[84请求验证与拓扑](builder-position/result-review.md)

上轮main并发是另一个真实且不同的控制：N256、每候选k16/64文件多点修改、salt不同、c1/2/4；每候选source/output/cache隔离，顺序seed后同时final。总CPU固定0…3，jobs=4/c。A与B实际是同一份main990 binary，A/A只作波动控制，不能充当候选B优势。[main并发独审](https://github.com/zil-lang/zil/blob/93adb1e7aec44bce6e93fcce237dbf77ec2b7aa4/research/compiler-multi-edit-20261111/multi-edit/main-concurrency-result-independent-audit.md)

| 已有c4观察 | k16 | k64 |
| --- | ---: | ---: |
| main批次mean wall（秒） | 6.172 | 7.852 |
| Rust批次mean wall（秒） | 3.641 | 4.082 |
| main任务/秒 | 0.648 | 0.509 |
| Rust任务/秒 | 1.099 | 0.980 |

36批/252次clean-seed-final编译与756次native功能输出完整；它显示当前main在所测固定预算下可提高吞吐，但每任务latency随c增加，Rust六对应格均更快。c4每批四个任务而c1一个，不能直接把batch wall当同工作量加速。仅一个block，A/A漂移−11.233%至+9.742%不能归因优化；Rust incremental ELF与clean不同已如实保留，不能说84个child全同码。[完整数据及口径](https://github.com/zil-lang/zil/blob/93adb1e7aec44bce6e93fcce237dbf77ec2b7aa4/research/compiler-multi-edit-20261111/multi-edit/main-concurrency-result-independent-audit.md)

它**没有**覆盖2321或2458，并非新native路径的多点／并发收益；独立cache也不测跨候选共享cold miss去重。旧warm c4的重复key都已object-cache命中，真正builder key各不相同，故已有数据不能证明single-flight会省重复builder。Blaze式共享daemon调度也尚未移植。[精确key只读核查](blaze-next.md#已存真实候选的重叠机会及不应声称的收益)

## 六、保持否定证据，收束本轮

已NoGo或排除的路径继续明确保留：layout投影收益混合、bounded imports损失跨leaf优化且运行1.616–2.123倍、历史completeAE四臂、CP084 absent-key/reverse-index、worker析构与前置合并等，不换名重做。默认Thin、strictAE、private wrapper、identity、linear与native2321各自冻结源/协议/raw/失败原因分开；后续private/native有真实新机制依据，不将它们当旧AE已经被推翻或笼统“ThinLTO成功”。[前轮归档结论](https://github.com/zil-lang/zil/blob/93adb1e7aec44bce6e93fcce237dbf77ec2b7aa4/research/compiler-multi-edit-20261111/multi-edit/continuation-results.md)、[AE历史](https://github.com/zil-lang/zil/blob/93adb1e7aec44bce6e93fcce237dbf77ec2b7aa4/research/compiler-multi-edit-20261111/multi-edit/available-imports-history-audit.md)

本轮收束只做证据与决策：Blaze代理已完成full独立raw审计，由既有冻结门分列功能、运行质量、编译wall/CPU和覆盖范围；完整NoGo原样保留，不补样、不改权重、不从诊断数值扣时间。统一索引精确remote/tree、source/build/binary/protocol/raw身份，并将NoGo、未执行和待定分开。本轮所有候选均不采用、不合main；未经已有证据覆盖的warm、多点、并发及整体频率门仍明确标“未证明”。本轮就此收束，不再新开变体、矩阵或实现。
